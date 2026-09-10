use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rust_decimal::Decimal;

use crate::diagnostics::{self, Event};
use crate::storage;

const SETTINGS_FILE: &str = "settings.toml";
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) threshold: Decimal,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            threshold: Decimal::ZERO,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadResult {
    pub(crate) settings: Settings,
    pub(crate) recovered: bool,
}

pub(crate) fn load() -> Result<LoadResult, SettingsError> {
    load_from_path(&settings_path()?)
}

fn load_from_path(path: &Path) -> Result<LoadResult, SettingsError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => match decode(&contents) {
            Ok(settings) => Ok(LoadResult {
                settings,
                recovered: false,
            }),
            Err(SettingsError::Decode | SettingsError::InvalidThreshold) => {
                recover_invalid_settings(path)
            }
            Err(error) => Err(error),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(LoadResult {
            settings: Settings::default(),
            recovered: false,
        }),
        Err(error) => Err(SettingsError::Io(error)),
    }
}

pub(crate) fn save(settings: &Settings) -> Result<(), SettingsError> {
    validate(settings)?;
    storage::write_atomically(&settings_path()?, encode(settings).as_bytes())
        .map_err(SettingsError::Io)
}

pub(crate) fn parse_threshold(value: &str) -> Result<Decimal, SettingsError> {
    let threshold = Decimal::from_str(value.trim()).map_err(|_| SettingsError::InvalidThreshold)?;
    validate(&Settings { threshold })?;
    Ok(threshold)
}

fn recover_invalid_settings(path: &Path) -> Result<LoadResult, SettingsError> {
    storage::quarantine_invalid_file(path).map_err(SettingsError::Io)?;
    diagnostics::record(Event::SettingsRecovered);
    Ok(LoadResult {
        settings: Settings::default(),
        recovered: true,
    })
}

fn settings_path() -> Result<PathBuf, SettingsError> {
    storage::data_dir()
        .map(|directory| directory.join(SETTINGS_FILE))
        .ok_or(SettingsError::UnavailablePath)
}

fn encode(settings: &Settings) -> String {
    format!(
        "version = {VERSION}\nthreshold = \"{}\"\n",
        settings.threshold
    )
}

fn decode(contents: &str) -> Result<Settings, SettingsError> {
    let mut version = None;
    let mut threshold = None;
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((key, value)) = line.split_once('=') else {
            return Err(SettingsError::Decode);
        };
        match key.trim() {
            "version" => version = value.trim().parse::<u32>().ok(),
            "threshold" => {
                threshold = Some(parse_threshold(value.trim().trim_matches('"'))?);
            }
            _ => return Err(SettingsError::Decode),
        }
    }
    if version != Some(VERSION) {
        return Err(SettingsError::Decode);
    }
    threshold
        .map(|threshold| Settings { threshold })
        .ok_or(SettingsError::Decode)
}

fn validate(settings: &Settings) -> Result<(), SettingsError> {
    if settings.threshold.is_sign_negative() {
        Err(SettingsError::InvalidThreshold)
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum SettingsError {
    UnavailablePath,
    Io(io::Error),
    Decode,
    InvalidThreshold,
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnavailablePath => {
                write!(formatter, "could not determine the macOS settings path")
            }
            Self::Io(error) => write!(formatter, "settings I/O error: {error}"),
            Self::Decode => write!(formatter, "settings file is invalid or unsupported"),
            Self::InvalidThreshold => write!(formatter, "threshold must be a non-negative decimal"),
        }
    }
}

impl std::error::Error for SettingsError {}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{Settings, SettingsError, decode, encode, load_from_path, parse_threshold};

    #[test]
    fn defaults_to_zero_threshold() {
        assert_eq!(Settings::default().threshold, Decimal::ZERO);
    }

    #[test]
    fn round_trips_a_settings_record() {
        let settings = Settings {
            threshold: Decimal::new(436_990, 2),
        };
        assert_eq!(decode(&encode(&settings)).unwrap(), settings);
    }

    #[test]
    fn rejects_unsupported_version_and_negative_thresholds() {
        assert!(matches!(
            decode("version = 2\nthreshold = \"0\"\n"),
            Err(SettingsError::Decode)
        ));
        assert!(matches!(
            parse_threshold("-0.01"),
            Err(SettingsError::InvalidThreshold)
        ));
    }

    #[test]
    fn recovers_unsupported_settings_and_preserves_a_quarantine_copy() {
        let directory = std::env::temp_dir().join(format!(
            "gold-ticker-settings-recovery-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.toml");
        std::fs::write(&path, "version = 2\nthreshold = \"0\"\n").unwrap();

        let loaded = load_from_path(&path).unwrap();

        assert!(loaded.recovered);
        assert_eq!(loaded.settings, Settings::default());
        assert!(!path.exists());
        assert!(std::fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("settings.toml.invalid-")
        }));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
