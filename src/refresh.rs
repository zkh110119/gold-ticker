use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::cache;
use crate::diagnostics::{self, Event};
use crate::provider::{Quote, TencentGoldProvider};

pub(crate) const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub(crate) const MANUAL_REFRESH_DEBOUNCE: Duration = Duration::from_secs(15);
const MAX_BACKOFF: Duration = Duration::from_secs(60 * 60);

#[derive(Debug)]
pub(crate) enum RefreshEvent {
    QuoteUpdated(Quote),
    FetchFailed(String),
}

#[derive(Debug, Clone)]
pub(crate) struct RefreshHandle {
    command_sender: Sender<RefreshCommand>,
}

impl RefreshHandle {
    pub(crate) fn request_manual_refresh(&self) -> bool {
        let accepted = self.command_sender.send(RefreshCommand::FetchNow).is_ok();
        if accepted {
            diagnostics::record(Event::ManualRefreshRequested);
        }
        accepted
    }
}

#[derive(Debug, Default)]
pub(crate) struct ManualRefreshDebounce {
    last_request: Option<Instant>,
}

impl ManualRefreshDebounce {
    pub(crate) fn try_accept(&mut self, now: Instant) -> bool {
        if self
            .last_request
            .is_some_and(|last_request| now.duration_since(last_request) < MANUAL_REFRESH_DEBOUNCE)
        {
            return false;
        }
        self.last_request = Some(now);
        true
    }
}

#[derive(Debug)]
enum RefreshCommand {
    FetchNow,
}

pub(crate) fn spawn() -> (RefreshHandle, Receiver<RefreshEvent>) {
    let (event_sender, event_receiver) = mpsc::channel();
    let (command_sender, command_receiver) = mpsc::channel();
    thread::Builder::new()
        .name("gold-ticker-refresh".to_owned())
        .spawn(move || refresh_loop(event_sender, command_receiver))
        .expect("Gold Ticker must be able to start its refresh worker");
    (RefreshHandle { command_sender }, event_receiver)
}

fn refresh_loop(sender: Sender<RefreshEvent>, command_receiver: Receiver<RefreshCommand>) {
    diagnostics::record(Event::RefreshWorkerStarted);
    let provider = match TencentGoldProvider::new(Duration::from_secs(10)) {
        Ok(provider) => provider,
        Err(error) => {
            diagnostics::record(Event::RefreshFailed);
            let _ = sender.send(RefreshEvent::FetchFailed(error.to_string()));
            diagnostics::record(Event::RefreshWorkerStopped);
            return;
        }
    };
    let mut consecutive_failures = 0;

    loop {
        match provider.fetch_quote() {
            Ok(quote) => {
                if let Err(error) = update_cache(quote.clone()) {
                    diagnostics::record(Event::RefreshFailed);
                    let _ = sender.send(RefreshEvent::FetchFailed(error));
                }
                if sender.send(RefreshEvent::QuoteUpdated(quote)).is_err() {
                    diagnostics::record(Event::RefreshWorkerStopped);
                    return;
                }
                diagnostics::record(Event::RefreshSucceeded);
                consecutive_failures = 0;
            }
            Err(error) => {
                consecutive_failures += 1;
                diagnostics::record(Event::RefreshFailed);
                if sender
                    .send(RefreshEvent::FetchFailed(error.to_string()))
                    .is_err()
                {
                    diagnostics::record(Event::RefreshWorkerStopped);
                    return;
                }
            }
        }

        match command_receiver.recv_timeout(next_delay(consecutive_failures)) {
            Ok(RefreshCommand::FetchNow) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                diagnostics::record(Event::RefreshWorkerStopped);
                return;
            }
        }
    }
}

fn update_cache(quote: Quote) -> Result<(), String> {
    let mut cache = cache::load().map_err(|error| error.to_string())?.cache;
    if cache.current.as_ref().is_some_and(|current| {
        current.symbol == quote.symbol && current.display_name == quote.display_name
    }) {
        cache.previous = cache.current.take();
    }
    cache.current = Some(quote);
    cache::save(&cache).map_err(|error| error.to_string())
}

pub(crate) fn next_delay(consecutive_failures: u32) -> Duration {
    if consecutive_failures == 0 {
        return DEFAULT_REFRESH_INTERVAL;
    }

    let multiplier = 1_u64 << consecutive_failures.saturating_sub(1).min(4);
    DEFAULT_REFRESH_INTERVAL
        .checked_mul(multiplier as u32)
        .unwrap_or(MAX_BACKOFF)
        .min(MAX_BACKOFF)
}

pub(crate) fn receive_pending(receiver: &Receiver<RefreshEvent>) -> Vec<RefreshEvent> {
    receiver.try_iter().collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::{
        DEFAULT_REFRESH_INTERVAL, MANUAL_REFRESH_DEBOUNCE, MAX_BACKOFF, ManualRefreshDebounce,
        RefreshCommand, next_delay,
    };

    #[test]
    fn uses_default_interval_after_success() {
        assert_eq!(next_delay(0), DEFAULT_REFRESH_INTERVAL);
    }

    #[test]
    fn applies_capped_exponential_backoff_after_failures() {
        assert_eq!(next_delay(1), Duration::from_secs(5 * 60));
        assert_eq!(next_delay(2), Duration::from_secs(10 * 60));
        assert_eq!(next_delay(3), Duration::from_secs(20 * 60));
        assert_eq!(next_delay(4), Duration::from_secs(40 * 60));
        assert_eq!(next_delay(5), MAX_BACKOFF);
        assert_eq!(next_delay(20), MAX_BACKOFF);
    }

    #[test]
    fn manual_refresh_debounce_accepts_exact_boundary() {
        let start = Instant::now();
        let mut debounce = ManualRefreshDebounce::default();
        assert!(debounce.try_accept(start));
        assert!(!debounce.try_accept(start + MANUAL_REFRESH_DEBOUNCE - Duration::from_nanos(1)));
        assert!(debounce.try_accept(start + MANUAL_REFRESH_DEBOUNCE));
    }

    #[test]
    fn manual_command_interrupts_the_wait_channel() {
        let (sender, receiver) = mpsc::channel();
        sender.send(RefreshCommand::FetchNow).unwrap();
        assert!(matches!(
            receiver.recv_timeout(Duration::ZERO),
            Ok(RefreshCommand::FetchNow)
        ));
    }

    #[test]
    fn disconnected_command_channel_is_observable_by_worker() {
        let (sender, receiver) = mpsc::channel::<RefreshCommand>();
        drop(sender);
        assert!(matches!(
            receiver.recv_timeout(Duration::ZERO),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}
