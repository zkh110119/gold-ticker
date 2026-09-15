use std::sync::mpsc::Sender;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNUserNotificationCenter,
};

#[derive(Debug)]
pub(crate) enum NotificationEvent {
    Delivered,
    Failed,
    UnavailableOutsideAppBundle,
}

pub(crate) fn request_threshold_authorization(sender: Sender<NotificationEvent>) {
    if !is_app_bundle_path(&NSBundle::mainBundle().bundlePath().to_string()) {
        let _ = sender.send(NotificationEvent::UnavailableOutsideAppBundle);
        return;
    }

    let center = UNUserNotificationCenter::currentNotificationCenter();
    let handler = RcBlock::new(move |granted: Bool, error: *mut NSError| {
        if !error.is_null() || !granted.as_bool() {
            let _ = sender.send(NotificationEvent::Failed);
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &handler,
    );
}

pub(crate) fn request_threshold_alert(
    sender: Sender<NotificationEvent>,
    price: &str,
    threshold: &str,
) {
    if !is_app_bundle_path(&NSBundle::mainBundle().bundlePath().to_string()) {
        let _ = sender.send(NotificationEvent::UnavailableOutsideAppBundle);
        return;
    }

    let center = UNUserNotificationCenter::currentNotificationCenter();
    let request_center = center.clone();
    let price = price.to_owned();
    let threshold = threshold.to_owned();
    let handler = RcBlock::new(move |granted: Bool, error: *mut NSError| {
        if !error.is_null() || !granted.as_bool() {
            let _ = sender.send(NotificationEvent::Failed);
            return;
        }

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str("GoldTicker 阈值提醒"));
        content.setBody(&NSString::from_str(&format!(
            "最新价格 {price} 已低于阈值 {threshold}"
        )));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str("gold-ticker-threshold-crossing"),
            &content,
            None,
        );
        let completion_sender = sender.clone();
        let completion = RcBlock::new(move |error: *mut NSError| {
            let event = if error.is_null() {
                NotificationEvent::Delivered
            } else {
                NotificationEvent::Failed
            };
            let _ = completion_sender.send(event);
        });
        request_center.addNotificationRequest_withCompletionHandler(&request, Some(&completion));
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &handler,
    );
}

fn is_app_bundle_path(path: &str) -> bool {
    path.ends_with(".app")
}

#[cfg(test)]
mod tests {
    use super::is_app_bundle_path;

    #[test]
    fn requires_a_mac_os_app_bundle_for_notifications() {
        assert!(is_app_bundle_path("/Applications/GoldTicker.app"));
        assert!(is_app_bundle_path("/tmp/GoldTicker.app"));
        assert!(!is_app_bundle_path("/project/target/debug"));
        assert!(!is_app_bundle_path("/project/target/release/gold-ticker"));
    }
}
