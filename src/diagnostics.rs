use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage;

const LOG_DIRECTORY: &str = "logs";
const LOG_FILE: &str = "gold-ticker.log";
const MAX_LOG_BYTES: u64 = 64 * 1024;
const MAX_LOG_FILES: usize = 4;
static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Copy)]
pub(crate) enum Event {
    AppStarted,
    CacheRecovered,
    SettingsRecovered,
    RefreshWorkerStarted,
    RefreshSucceeded,
    RefreshFailed,
    ManualRefreshRequested,
    RefreshWorkerStopped,
}

impl Event {
    fn message(self) -> &'static str {
        match self {
            Self::AppStarted => "app_started",
            Self::CacheRecovered => "cache_recovered",
            Self::SettingsRecovered => "settings_recovered",
            Self::RefreshWorkerStarted => "refresh_worker_started",
            Self::RefreshSucceeded => "refresh_succeeded",
            Self::RefreshFailed => "refresh_failed",
            Self::ManualRefreshRequested => "manual_refresh_requested",
            Self::RefreshWorkerStopped => "refresh_worker_stopped",
        }
    }
}

pub(crate) fn record(event: Event) {
    let Some(data_directory) = storage::data_dir() else {
        return;
    };
    let _ = write_event(&data_directory, event);
}

fn write_event(data_directory: &Path, event: Event) -> io::Result<()> {
    let _lock = WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("diagnostics lock poisoned");
    let log_directory = data_directory.join(LOG_DIRECTORY);
    fs::create_dir_all(&log_directory)?;
    rotate_if_needed(&log_directory)?;
    let log_path = log_directory.join(LOG_FILE);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let entry = format!("{timestamp} {}\n", event.message());
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?
        .write_all(entry.as_bytes())
}

fn rotate_if_needed(log_directory: &Path) -> io::Result<()> {
    let current = log_directory.join(LOG_FILE);
    if current
        .metadata()
        .is_ok_and(|metadata| metadata.len() >= MAX_LOG_BYTES)
    {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        fs::rename(
            &current,
            log_directory.join(format!("gold-ticker-{timestamp}.log")),
        )?;
    }
    prune_old_logs(log_directory)
}

fn prune_old_logs(log_directory: &Path) -> io::Result<()> {
    let mut logs = fs::read_dir(log_directory)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("gold-ticker-")
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "log")
        })
        .collect::<Vec<_>>();
    logs.sort_by_key(|entry| entry.file_name());
    while logs.len() >= MAX_LOG_FILES {
        fs::remove_file(logs.remove(0).path())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{Event, MAX_LOG_FILES, write_event};

    fn temporary_directory() -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "gold-ticker-diagnostics-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn writes_redacted_event_names_only() {
        let directory = temporary_directory();
        write_event(&directory, Event::RefreshFailed).unwrap();
        let contents = fs::read_to_string(directory.join("logs/gold-ticker.log")).unwrap();

        assert!(contents.contains("refresh_failed"));
        assert!(!contents.contains("threshold"));
        assert!(!contents.contains("v_hf_XAU"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retains_only_a_bounded_number_of_rotated_logs() {
        let directory = temporary_directory();
        let log_directory = directory.join("logs");
        fs::create_dir_all(&log_directory).unwrap();
        for index in 0..=MAX_LOG_FILES {
            fs::write(
                log_directory.join(format!("gold-ticker-{index}.log")),
                "old",
            )
            .unwrap();
        }

        write_event(&directory, Event::AppStarted).unwrap();

        let count = fs::read_dir(log_directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".log"))
            .count();
        assert!(count <= MAX_LOG_FILES);
        fs::remove_dir_all(directory).unwrap();
    }
}
