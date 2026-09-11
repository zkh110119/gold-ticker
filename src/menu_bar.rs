use std::cell::{OnceCell, RefCell};
use std::sync::mpsc::{self, Receiver};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApp, NSAppearanceCustomization, NSApplication, NSApplicationActivationPolicy,
    NSApplicationDelegate, NSButton, NSButtonType, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSEventType, NSMenu, NSMenuItem, NSPopover, NSPopoverBehavior,
    NSStatusBar, NSStatusItem, NSTextField, NSVariableStatusItemLength, NSView, NSViewController,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSRectEdge,
    NSSize, NSString, NSTimer,
};

use crate::alerts::{self, Transition};
use crate::cache::{self, QuoteCache};
use crate::diagnostics::{self, Event};
use crate::domain::{self, Direction, Freshness, QuoteState, ThresholdStatus};
use crate::formatting;
use crate::macos_notifications::{self, NotificationEvent};
use crate::provider::Quote;
use crate::refresh::{self, ManualRefreshDebounce, RefreshEvent, RefreshHandle};
use crate::settings::{self, Settings};

const POPOVER_SIZE: NSSize = NSSize::new(340.0, 306.0);
const UNSIGNED_LOGIN_ITEM_MESSAGE: &str = "当前未签名版本无法启用开机启动";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuotePopoverBehavior {
    Transient,
}

impl QuotePopoverBehavior {
    fn app_kit(self) -> NSPopoverBehavior {
        match self {
            Self::Transient => NSPopoverBehavior::Transient,
        }
    }
}

const QUOTE_POPOVER_BEHAVIOR: QuotePopoverBehavior = QuotePopoverBehavior::Transient;

#[derive(Debug)]
struct PopoverViews {
    headline: Retained<NSTextField>,
    price: Retained<NSTextField>,
    change: Retained<NSTextField>,
    threshold: Retained<NSTextField>,
    detail: Retained<NSTextField>,
    threshold_input: Retained<NSTextField>,
    launch_at_login: Retained<NSButton>,
    login_status: Retained<NSTextField>,
    refresh_button: Retained<NSButton>,
}

#[derive(Default)]
struct AppDelegateIvars {
    status_item: OnceCell<Retained<NSStatusItem>>,
    context_menu: OnceCell<Retained<NSMenu>>,
    popover: OnceCell<Retained<NSPopover>>,
    popover_views: OnceCell<PopoverViews>,
    refresh_receiver: RefCell<Option<Receiver<RefreshEvent>>>,
    notification_receiver: RefCell<Option<Receiver<NotificationEvent>>>,
    notification_sender: OnceCell<mpsc::Sender<NotificationEvent>>,
    refresh_handle: OnceCell<RefreshHandle>,
    refresh_timer: OnceCell<Retained<NSTimer>>,
    cache: RefCell<QuoteCache>,
    settings: RefCell<Settings>,
    last_error: RefCell<Option<String>>,
    is_refreshing: RefCell<bool>,
    manual_refresh_debounce: RefCell<ManualRefreshDebounce>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and Delegate does not implement Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppDelegateIvars]
    struct AppDelegate;

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for AppDelegate {}

    // SAFETY: NSApplicationDelegate has no safety requirements.
    unsafe impl NSApplicationDelegate for AppDelegate {
        // SAFETY: The Objective-C selector signature is correct.
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            diagnostics::record(Event::AppStarted);
            let mtm = self.mtm();
            let application = NSApp(mtm);
            application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

            let status_bar = NSStatusBar::systemStatusBar();
            let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
            let button = status_item
                .button(mtm)
                .expect("a newly created status item must have a button");
            button.setTitle(&NSString::from_str("Au 更新中…"));
            button.setToolTip(Some(&NSString::from_str("正在请求最新行情")));
            let context_menu = build_context_menu(mtm, self);
            // SAFETY: AppDelegate outlives the status-item button and the selector has the expected signature.
            unsafe {
                button.setTarget(Some(self));
                button.setAction(Some(sel!(togglePopover:)));
                button.sendActionOn(
                    objc2_app_kit::NSEventMask::LeftMouseUp
                        | objc2_app_kit::NSEventMask::RightMouseUp,
                );
            }

            let (popover, popover_views) = build_popover(mtm, self);
            let (refresh_handle, refresh_receiver) = refresh::spawn();
            let (notification_sender, notification_receiver) = mpsc::channel();
            let loaded_cache = cache::load();
            let loaded_settings = settings::load();

            self.ivars()
                .context_menu
                .set(context_menu)
                .expect("application delegate must only configure the context menu once");
            self.ivars()
                .status_item
                .set(status_item)
                .expect("application delegate must only launch once");
            self.ivars()
                .popover
                .set(popover)
                .expect("application delegate must only launch once");
            self.ivars()
                .popover_views
                .set(popover_views)
                .expect("application delegate must only launch once");
            self.ivars()
                .refresh_handle
                .set(refresh_handle)
                .expect("application delegate must only launch once");
            self.ivars().refresh_receiver.replace(Some(refresh_receiver));
            self.ivars()
                .notification_sender
                .set(notification_sender)
                .expect("notification sender must only be configured once");
            self.ivars()
                .notification_receiver
                .replace(Some(notification_receiver));

            match loaded_cache {
                Ok(loaded) => {
                    if loaded.recovered {
                        self.ivars()
                            .last_error
                            .replace(Some("行情缓存无效，已恢复默认缓存".to_owned()));
                    }
                    self.ivars().cache.replace(loaded.cache);
                }
                Err(error) => {
                    self.ivars()
                        .last_error
                        .replace(Some(format!("无法读取行情缓存：{error}")));
                }
            }
            match loaded_settings {
                Ok(loaded) => {
                    if loaded.recovered {
                        self.ivars()
                            .last_error
                            .replace(Some("阈值设置无效，已恢复默认阈值".to_owned()));
                    }
                    self.ivars().settings.replace(loaded.settings);
                }
                Err(error) => {
                    self.ivars()
                        .last_error
                        .replace(Some(format!("无法读取阈值设置：{error}")));
                }
            }
            let settings = self.ivars().settings.borrow();
            let threshold = settings.threshold;
            let launch_at_login = settings.launch_at_login;
            drop(settings);
            let views = self
                .ivars()
                .popover_views
                .get()
                .expect("popover views must exist after application launch");
            views
                .threshold_input
                .setStringValue(&NSString::from_str(&formatting::format_price(threshold)));
            views.launch_at_login.setState(if launch_at_login {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            render(self);

            // SAFETY: AppDelegate remains alive for the application's lifetime,
            // and the selector accepts the NSTimer sender argument.
            let refresh_timer = unsafe {
                NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    1.0,
                    self,
                    sel!(pollRefresh:),
                    None,
                    true,
                )
            };
            self.ivars()
                .refresh_timer
                .set(refresh_timer)
                .expect("refresh timer must only be configured once");
        }
    }

    impl AppDelegate {
        // SAFETY: The selector has the expected `NSTimer -> void` Objective-C signature.
        #[unsafe(method(pollRefresh:))]
        fn poll_refresh(&self, _timer: Option<&NSTimer>) {
            let events = {
                let receiver = self.ivars().refresh_receiver.borrow();
                let Some(receiver) = receiver.as_ref() else {
                    return;
                };
                refresh::receive_pending(receiver)
            };

            for event in events {
                match event {
                    RefreshEvent::QuoteUpdated(quote) => {
                        if let Err(error) = update_cache_state(self, quote) {
                            self.ivars().last_error.replace(Some(error));
                        } else {
                            self.ivars().last_error.replace(None);
                        }
                        self.ivars().is_refreshing.replace(false);
                    }
                    RefreshEvent::FetchFailed(message) => {
                        self.ivars().last_error.replace(Some(message));
                        self.ivars().is_refreshing.replace(false);
                    }
                }
            }
            let notification_events = {
                let receiver = self.ivars().notification_receiver.borrow();
                receiver
                    .as_ref()
                    .map_or_else(Vec::new, |receiver| receiver.try_iter().collect())
            };
            for event in notification_events {
                match event {
                    NotificationEvent::Delivered => diagnostics::record(Event::ThresholdAlertDelivered),
                    NotificationEvent::Failed => {
                        diagnostics::record(Event::ThresholdAlertFailed);
                        self.ivars()
                            .last_error
                            .replace(Some("阈值提醒未能发送；请检查通知权限".to_owned()));
                    }
                    NotificationEvent::UnavailableOutsideAppBundle => {
                        diagnostics::record(Event::ThresholdAlertUnavailable);
                        self.ivars()
                            .last_error
                            .replace(Some("开发模式不支持阈值提醒；请从 GoldTicker.app 启动".to_owned()));
                    }
                }
            }
            render(self);
        }

        // SAFETY: The selector has the expected `id -> void` Objective-C signature.
        #[unsafe(method(togglePopover:))]
        fn toggle_popover(&self, _sender: Option<&AnyObject>) {
            let mtm = self.mtm();
            let application = NSApp(mtm);
            if application
                .currentEvent()
                .is_some_and(|event| event.r#type() == NSEventType::RightMouseUp)
            {
                if let Some(menu) = self.ivars().context_menu.get() {
                    let status_item = self
                        .ivars()
                        .status_item
                        .get()
                        .expect("status item must exist after application launch");
                    let button = status_item
                        .button(mtm)
                        .expect("status item must have a button");
                    if let Some(event) = application.currentEvent() {
                        sync_context_menu_appearance(mtm, menu);
                        NSMenu::popUpContextMenu_withEvent_forView(menu, &event, &button);
                    }
                }
                return;
            }
            let status_item = self
                .ivars()
                .status_item
                .get()
                .expect("status item must exist after application launch");
            let button = status_item
                .button(mtm)
                .expect("status item must have a button");
            let popover = self
                .ivars()
                .popover
                .get()
                .expect("popover must exist after application launch");

            if popover.isShown() {
                popover.close();
            } else {
                popover.showRelativeToRect_ofView_preferredEdge(
                    button.bounds(),
                    &button,
                    NSRectEdge::MinY,
                );
            }
        }

        // SAFETY: The selector has the expected `id -> void` Objective-C signature.
        #[unsafe(method(quitApplication:))]
        fn quit_application(&self, _sender: Option<&AnyObject>) {
            NSApp(self.mtm()).terminate(None);
        }

        // SAFETY: The selector has the expected `id -> void` Objective-C signature.
        #[unsafe(method(requestManualRefresh:))]
        fn request_manual_refresh(&self, _sender: Option<&AnyObject>) {
            if !self
                .ivars()
                .manual_refresh_debounce
                .borrow_mut()
                .try_accept(Instant::now())
            {
                self.ivars()
                    .last_error
                    .replace(Some("请等待 15 秒后再次刷新".to_owned()));
                render(self);
                return;
            }
            if self
                .ivars()
                .refresh_handle
                .get()
                .expect("refresh handle must exist after application launch")
                .request_manual_refresh()
            {
                self.ivars().is_refreshing.replace(true);
                self.ivars().last_error.replace(None);
            } else {
                self.ivars()
                    .last_error
                    .replace(Some("刷新服务不可用；请重新启动应用".to_owned()));
            }
            render(self);
        }

        // SAFETY: The selector has the expected `id -> void` Objective-C signature.
        #[unsafe(method(saveThreshold:))]
        fn save_threshold(&self, _sender: Option<&AnyObject>) {
            let views = self
                .ivars()
                .popover_views
                .get()
                .expect("popover views must exist after application launch");
            let value = views.threshold_input.stringValue().to_string();
            match settings::parse_threshold(&value) {
                Ok(threshold) => {
                    let previous_settings = self.ivars().settings.borrow().clone();
                    let baseline_status = self
                        .ivars()
                        .cache
                        .borrow()
                        .current
                        .as_ref()
                        .map(|quote| domain::threshold_status(quote.price, threshold));
                    let new_settings = Settings {
                        threshold,
                        last_threshold_status: baseline_status,
                        ..previous_settings
                    };
                    match settings::save(&new_settings) {
                        Ok(()) => {
                            views.threshold_input.setStringValue(&NSString::from_str(
                                &formatting::format_price(threshold),
                            ));
                            self.ivars().settings.replace(new_settings);
                            self.ivars().last_error.replace(None);
                            macos_notifications::request_threshold_authorization(
                                self.ivars()
                                    .notification_sender
                                    .get()
                                    .expect("notification sender must exist after application launch")
                                    .clone(),
                            );
                        }
                        Err(error) => {
                            self.ivars()
                                .last_error
                                .replace(Some(format!("无法保存阈值：{error}")));
                        }
                    }
                }
                Err(error) => {
                    self.ivars()
                        .last_error
                        .replace(Some(format!("阈值无效：{error}")));
                }
            }
            render(self);
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppDelegateIvars::default());
        // SAFETY: NSObject's init method has the expected signature.
        unsafe { msg_send![super(this), init] }
    }
}

fn update_cache_state(delegate: &AppDelegate, quote: Quote) -> Result<(), String> {
    let mut settings = delegate.ivars().settings.borrow_mut();
    let threshold_status = domain::threshold_status(quote.price, settings.threshold);
    let alert = alerts::transition(settings.last_threshold_status, threshold_status);
    settings.last_threshold_status = Some(threshold_status);
    settings::save(&settings).map_err(|error| format!("无法保存提醒状态：{error}"))?;
    let threshold = settings.threshold;
    drop(settings);

    let mut cache = delegate.ivars().cache.borrow_mut();
    if cache.current.as_ref().is_some_and(|current| {
        current.symbol == quote.symbol && current.display_name == quote.display_name
    }) {
        cache.previous = cache.current.take();
    }
    cache.current = Some(quote.clone());
    drop(cache);

    if alert == Transition::CrossedBelow {
        diagnostics::record(Event::ThresholdAlertRequested);
        macos_notifications::request_threshold_alert(
            delegate
                .ivars()
                .notification_sender
                .get()
                .expect("notification sender must exist after application launch")
                .clone(),
            &formatting::format_price(quote.price),
            &formatting::format_price(threshold),
        );
    }
    Ok(())
}

fn render(delegate: &AppDelegate) {
    let now_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let settings = delegate.ivars().settings.borrow().clone();
    let cache = delegate.ivars().cache.borrow().clone();
    let error = delegate.ivars().last_error.borrow().clone();
    let is_refreshing = *delegate.ivars().is_refreshing.borrow();
    let state = cache.current.map(|current| {
        domain::quote_state(
            current,
            cache.previous.as_ref(),
            settings.threshold,
            now_unix_secs,
        )
    });

    render_status_item(delegate, state.as_ref(), error.as_deref(), is_refreshing);
    render_popover(
        delegate,
        state.as_ref(),
        settings.threshold,
        error.as_deref(),
        is_refreshing,
    );
}

fn render_status_item(
    delegate: &AppDelegate,
    state: Option<&QuoteState>,
    error: Option<&str>,
    is_refreshing: bool,
) {
    let status_item = delegate
        .ivars()
        .status_item
        .get()
        .expect("status item must exist after application launch");
    let button = status_item
        .button(delegate.mtm())
        .expect("status item must have a button");
    match state {
        Some(state) => {
            let stale = state.freshness == Freshness::Stale;
            button.setTitle(&NSString::from_str(&formatting::menu_bar_title(
                state.quote.price,
                state.change.as_ref(),
                stale,
            )));
            let freshness = if stale {
                "数据可能已过期"
            } else {
                "正常"
            };
            let error_detail = error.map_or(String::new(), |message| format!("\n错误：{message}"));
            button.setToolTip(Some(&NSString::from_str(&format!(
                "{} · {}\n{}\n行情时间：{} {}\n数据源：{}\n状态：{freshness}{error_detail}",
                state.quote.display_name,
                state.quote.symbol,
                formatting::format_change(state.change.as_ref()),
                state.quote.quote_date,
                state.quote.quote_time,
                state.quote.provider,
            ))));
        }
        None => {
            button.setTitle(&NSString::from_str(if is_refreshing {
                "Au 更新中…"
            } else {
                "Au —"
            }));
            button.setToolTip(Some(&NSString::from_str(error.unwrap_or("尚无可用行情"))));
        }
    }
}

fn render_popover(
    delegate: &AppDelegate,
    state: Option<&QuoteState>,
    threshold: rust_decimal::Decimal,
    error: Option<&str>,
    is_refreshing: bool,
) {
    let views = delegate
        .ivars()
        .popover_views
        .get()
        .expect("popover views must exist after application launch");
    views
        .refresh_button
        .setTitle(&NSString::from_str(if is_refreshing {
            "刷新中…"
        } else {
            "立即刷新"
        }));
    views.refresh_button.setEnabled(!is_refreshing);
    views.launch_at_login.setEnabled(false);
    views
        .login_status
        .setStringValue(&NSString::from_str(UNSIGNED_LOGIN_ITEM_MESSAGE));
    views
        .login_status
        .setTextColor(Some(&NSColor::secondaryLabelColor()));

    match state {
        Some(state) => render_quote_popover(views, state, threshold, error),
        None => {
            views
                .headline
                .setStringValue(&NSString::from_str("国际现货黄金 · hf_XAU"));
            views.price.setStringValue(&NSString::from_str("—"));
            views
                .change
                .setStringValue(&NSString::from_str("暂无可比较行情"));
            views.threshold.setStringValue(&NSString::from_str(&format!(
                "阈值：{} 美元",
                formatting::format_price(threshold)
            )));
            views
                .detail
                .setStringValue(&NSString::from_str(error.unwrap_or(if is_refreshing {
                    "正在请求最新行情"
                } else {
                    "尚无可用行情；可点击立即刷新重试"
                })));
            views
                .price
                .setTextColor(Some(&NSColor::secondaryLabelColor()));
            views
                .change
                .setTextColor(Some(&NSColor::secondaryLabelColor()));
            views
                .threshold
                .setTextColor(Some(&NSColor::secondaryLabelColor()));
            views
                .detail
                .setTextColor(Some(&NSColor::secondaryLabelColor()));
        }
    }
}

fn render_quote_popover(
    views: &PopoverViews,
    state: &QuoteState,
    threshold: rust_decimal::Decimal,
    error: Option<&str>,
) {
    views.headline.setStringValue(&NSString::from_str(&format!(
        "{} · {}",
        state.quote.display_name, state.quote.symbol
    )));
    views
        .price
        .setStringValue(&NSString::from_str(&formatting::format_price(
            state.quote.price,
        )));
    views
        .change
        .setStringValue(&NSString::from_str(&formatting::format_change(
            state.change.as_ref(),
        )));
    let (threshold_text, price_color) = match state.threshold_status {
        ThresholdStatus::AtOrAbove => (
            format!(
                "阈值：{} 美元 · 当前高于或等于阈值",
                formatting::format_price(threshold)
            ),
            NSColor::systemRedColor(),
        ),
        ThresholdStatus::Below => (
            format!(
                "阈值：{} 美元 · 当前低于阈值",
                formatting::format_price(threshold)
            ),
            NSColor::systemGreenColor(),
        ),
    };
    views
        .threshold
        .setStringValue(&NSString::from_str(&threshold_text));
    let change_color = match state.change.as_ref().map(|change| change.direction) {
        Some(Direction::Up) => NSColor::systemRedColor(),
        Some(Direction::Down) => NSColor::systemGreenColor(),
        _ => NSColor::secondaryLabelColor(),
    };
    let freshness = if state.freshness == Freshness::Stale {
        "数据可能已过期"
    } else {
        "正常"
    };
    let error_detail = error.map_or(String::new(), |message| format!("\n错误：{message}"));
    views.detail.setStringValue(&NSString::from_str(&format!(
        "行情时间：{} {}\n数据源：{} · 币种与单位待确认\n状态：{freshness}{error_detail}",
        state.quote.quote_date, state.quote.quote_time, state.quote.provider,
    )));
    views.price.setTextColor(Some(&price_color));
    views.threshold.setTextColor(Some(&price_color));
    views.change.setTextColor(Some(&change_color));
    let detail_color = if state.freshness == Freshness::Stale || error.is_some() {
        NSColor::secondaryLabelColor()
    } else {
        NSColor::labelColor()
    };
    views.detail.setTextColor(Some(&detail_color));
}

fn sync_context_menu_appearance(mtm: MainThreadMarker, menu: &NSMenu) {
    let appearance = NSApp(mtm).effectiveAppearance();
    menu.setAppearance(Some(&appearance));
}

fn build_context_menu(mtm: MainThreadMarker, delegate: &AppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    sync_context_menu_appearance(mtm, &menu);
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str("退出 Gold Ticker"),
            Some(sel!(quitApplication:)),
            &NSString::from_str(""),
        )
    };
    // SAFETY: AppDelegate outlives the menu item and implements the selector.
    unsafe { item.setTarget(Some(delegate)) };
    menu.addItem(&item);
    menu
}

fn build_popover(
    mtm: MainThreadMarker,
    delegate: &AppDelegate,
) -> (Retained<NSPopover>, PopoverViews) {
    let content_view = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), POPOVER_SIZE),
    );
    let headline = label(mtm, "国际现货黄金 · hf_XAU", 16.0, 266.0, 308.0, 20.0);
    let price = label(mtm, "—", 16.0, 226.0, 308.0, 28.0);
    let change = label(mtm, "暂无可比较行情", 16.0, 198.0, 308.0, 20.0);
    let threshold = label(
        mtm,
        "阈值：4,000 美元 · 当前高于或等于阈值",
        16.0,
        170.0,
        308.0,
        20.0,
    );
    let detail = wrapping_label(mtm, "正在请求最新行情", 16.0, 112.0, 308.0, 48.0);
    let launch_at_login = checkbox(mtm, "开机启动", 16.0, 78.0, 110.0);
    launch_at_login.setEnabled(false);
    let login_status = label(mtm, UNSIGNED_LOGIN_ITEM_MESSAGE, 16.0, 58.0, 308.0, 16.0);
    login_status.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let threshold_input = NSTextField::textFieldWithString(&NSString::from_str("4,000"), mtm);
    threshold_input.setFrame(NSRect::new(
        NSPoint::new(16.0, 22.0),
        NSSize::new(130.0, 24.0),
    ));
    let save_button = button(
        mtm,
        delegate,
        "保存阈值",
        sel!(saveThreshold:),
        154.0,
        22.0,
        82.0,
    );
    let refresh_button = button(
        mtm,
        delegate,
        "立即刷新",
        sel!(requestManualRefresh:),
        242.0,
        22.0,
        82.0,
    );

    for view in [
        headline.as_ref(),
        price.as_ref(),
        change.as_ref(),
        threshold.as_ref(),
        detail.as_ref(),
        launch_at_login.as_ref(),
        login_status.as_ref(),
        threshold_input.as_ref(),
        save_button.as_ref(),
        refresh_button.as_ref(),
    ] {
        content_view.addSubview(view);
    }

    let controller = NSViewController::new(mtm);
    controller.setView(&content_view);
    let popover = NSPopover::new(mtm);
    popover.setBehavior(QUOTE_POPOVER_BEHAVIOR.app_kit());
    popover.setContentSize(POPOVER_SIZE);
    popover.setContentViewController(Some(&controller));
    (
        popover,
        PopoverViews {
            headline,
            price,
            change,
            threshold,
            detail,
            threshold_input,
            launch_at_login,
            login_status,
            refresh_button,
        },
    )
}

fn label(
    mtm: MainThreadMarker,
    value: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Retained<NSTextField> {
    let text = NSTextField::labelWithString(&NSString::from_str(value), mtm);
    text.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)));
    text
}

fn wrapping_label(
    mtm: MainThreadMarker,
    value: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Retained<NSTextField> {
    let text = NSTextField::wrappingLabelWithString(&NSString::from_str(value), mtm);
    text.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)));
    text
}

fn checkbox(mtm: MainThreadMarker, title: &str, x: f64, y: f64, width: f64) -> Retained<NSButton> {
    let checkbox = unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str(title), None, None, mtm)
    };
    checkbox.setButtonType(NSButtonType::Switch);
    checkbox.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(width, 20.0)));
    checkbox
}

fn button(
    mtm: MainThreadMarker,
    delegate: &AppDelegate,
    title: &str,
    action: objc2::runtime::Sel,
    x: f64,
    y: f64,
    width: f64,
) -> Retained<NSButton> {
    // SAFETY: AppDelegate outlives the button and each selector has the expected signature.
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(delegate),
            Some(action),
            mtm,
        )
    };
    button.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(width, 24.0)));
    button
}

pub(crate) fn run() -> ! {
    let mtm = MainThreadMarker::new().expect("Gold Ticker must start on the main thread");
    let application = NSApplication::sharedApplication(mtm);
    let delegate = AppDelegate::new(mtm);

    application.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    application.run();

    unreachable!("NSApplication run loop must not return");
}

#[cfg(test)]
mod tests {
    use super::{QUOTE_POPOVER_BEHAVIOR, QuotePopoverBehavior};
    use objc2_app_kit::NSPopoverBehavior;

    #[test]
    fn quote_popover_uses_transient_behavior_for_outside_click_dismissal() {
        assert_eq!(QUOTE_POPOVER_BEHAVIOR, QuotePopoverBehavior::Transient);
        assert_eq!(
            QUOTE_POPOVER_BEHAVIOR.app_kit(),
            NSPopoverBehavior::Transient
        );
    }
}
