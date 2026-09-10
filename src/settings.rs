use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rust_decimal::Decimal;

use crate::diagnostics::{self, Event};
use crate::domain::ThresholdStatus;
use crate::storage;

const SETTINGS_FILE: &str = "settings.toml";
const VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) threshold: Decimal,
    pub(crate) launch_at_login: bool,
    pub(crate) last_threshold_status: Option<ThresholdStatus>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            threshold: Decimal::ZERO,
            launch_at_login: true,
            last_threshold_status: None,
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
    let normalized = normalize_threshold_input(value.trim())?;
    let threshold = Decimal::from_str(&normalized).map_err(|_| SettingsError::InvalidThreshold)?;
    validate(&Settings {
        threshold,
        ..Settings::default()
    })?;
    Ok(threshold)
}

fn normalize_threshold_input(value: &str) -> Result<String, SettingsError> {
    let Some((integer, fraction)) = value.split_once('.') else {
        return normalize_grouped_integer(value);
    };

    if fraction.is_empty() || fraction.contains(',') {
        return Err(SettingsError::InvalidThreshold);
    }
    let normalized_integer = normalize_grouped_integer(integer)?;
    Ok(format!("{normalized_integer}.{fraction}"))
}

fn normalize_grouped_integer(value: &str) -> Result<String, SettingsError> {
    let (sign, digits) = value
        .strip_prefix('-')
        .map_or(("", value), |digits| ("-", digits));
    if !digits.contains(',') {
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(SettingsError::InvalidThreshold);
        }
        return Ok(format!("{sign}{digits}"));
    }

    let Some(first_group) = digits.split(',').next() else {
        return Err(SettingsError::InvalidThreshold);
    };
    if first_group.is_empty()
        || first_group.len() > 3
        || !first_group.chars().all(|c| c.is_ascii_digit())
    {
        return Err(SettingsError::InvalidThreshold);
    }

    let mut groups = digits.split(',');
    let _ = groups.next();
    for group in groups {
        if group.len() != 3 || !group.chars().all(|c| c.is_ascii_digit()) {
            return Err(SettingsError::InvalidThreshold);
        }
    }
    Ok(format!("{sign}{}", digits.replace(',', "")))
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
        "version = {VERSION}\nthreshold = \"{}\"\nlaunch_at_login = {}\nlast_threshold_status = \"{}\"\n",
        settings.threshold,
        settings.launch_at_login,
        encode_threshold_status(settings.last_threshold_status),
    )
}

fn encode_threshold_status(status: Option<ThresholdStatus>) -> &'static str {
    match status {
        None => "unknown",
        Some(ThresholdStatus::AtOrAbove) => "at_or_above",
        Some(ThresholdStatus::Below) => "below",
    }
}

fn decode(contents: &str) -> Result<Settings, SettingsError> {
    let mut version = None;
    let mut threshold = None;
    let mut launch_at_login = None;
    let mut last_threshold_status = None;
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
            "threshold" => threshold = Some(parse_threshold(value.trim().trim_matches('"'))?),
            "launch_at_login" => launch_at_login = value.trim().parse::<bool>().ok(),
            "last_threshold_status" => {
                last_threshold_status =
                    Some(decode_threshold_status(value.trim().trim_matches('"'))?)
            }
            _ => return Err(SettingsError::Decode),
        }
    }

    let threshold = threshold.ok_or(SettingsError::Decode)?;
    match version {
        Some(1) => Ok(Settings {
            threshold,
            ..Settings::default()
        }),
        Some(VERSION) => Ok(Settings {
            threshold,
            launch_at_login: launch_at_login.ok_or(SettingsError::Decode)?,
            last_threshold_status: last_threshold_status.ok_or(SettingsError::Decode)?,
        }),
        _ => Err(SettingsError::Decode),
    }
}

fn decode_threshold_status(value: &str) -> Result<Option<ThresholdStatus>, SettingsError> {
    match value {
        "unknown" => Ok(None),
        "at_or_above" => Ok(Some(ThresholdStatus::AtOrAbove)),
        "below" => Ok(Some(ThresholdStatus::Below)),
        _ => Err(SettingsError::Decode),
    }
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

    use crate::domain::ThresholdStatus;

    use super::{Settings, SettingsError, decode, encode, load_from_path, parse_threshold};

    #[test]
    fn defaults_to_zero_threshold() {
        assert_eq!(Settings::default().threshold, Decimal::ZERO);
    }

    #[test]
    fn round_trips_a_settings_record() {
        let settings = Settings {
            threshold: Decimal::new(436_990, 2),
            launch_at_login: false,
            last_threshold_status: Some(ThresholdStatus::Below),
        };
        assert_eq!(decode(&encode(&settings)).unwrap(), settings);
    }

    #[test]
    fn migrates_a_v1_settings_record_to_safe_r7_defaults() {
        assert_eq!(
            decode("version = 1\nthreshold = \"4200\"\n").unwrap(),
            Settings {
                threshold: Decimal::new(4200, 0),
                ..Settings::default()
            }
        );
    }

    #[test]
    fn accepts_valid_grouped_thresholds() {
        assert_eq!(parse_threshold("4,201").unwrap(), Decimal::new(4201, 0));
        assert_eq!(
            parse_threshold("4,201.50").unwrap(),
            Decimal::new(420150, 2)
        );
        assert_eq!(
            parse_threshold("12,345,678.900").unwrap(),
            Decimal::new(1_234_567_890, 2)
        );
    }

    #[test]
    fn rejects_malformed_grouping_and_fraction_separators() {
        for value in [
            ",4201",
            "42,01",
            "420,1",
            "4,20,100",
            "4,,201",
            "4,201,",
            "4,201.5,0",
            "4,201.",
        ] {
            assert!(
                matches!(parse_threshold(value), Err(SettingsError::InvalidThreshold)),
                "expected invalid threshold: {value}"
            );
        }
    }
    #[test]
    fn rejects_unsupported_version_and_negative_thresholds() {
        assert!(matches!(
            decode("version = 3\nthreshold = \"0\"\n"),
            Err(SettingsError::Decode)
        ));
        assert!(matches!(
            decode(
                "version = 2\nthreshold = \"0\"\nlaunch_at_login = true\nlast_threshold_status = \"invalid\"\n"
            ),
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
        std::fs::write(&path, "version = 3\nthreshold = \"0\"\n").unwrap();

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
