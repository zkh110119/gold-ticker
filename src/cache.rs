use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::diagnostics::{self, Event};
use crate::provider::Quote;
use crate::storage;

const CACHE_FILE: &str = "quote-cache.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QuoteCache {
    pub(crate) current: Option<Quote>,
    pub(crate) previous: Option<Quote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadResult {
    pub(crate) cache: QuoteCache,
    pub(crate) recovered: bool,
}

pub(crate) fn load() -> Result<LoadResult, CacheError> {
    load_from_path(&cache_path()?)
}

fn load_from_path(path: &std::path::Path) -> Result<LoadResult, CacheError> {
    match std::fs::read(path) {
        Ok(contents) => match serde_json::from_slice(&contents) {
            Ok(cache) => Ok(LoadResult {
                cache,
                recovered: false,
            }),
            Err(_) => recover_invalid_cache(path),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(LoadResult {
            cache: QuoteCache::default(),
            recovered: false,
        }),
        Err(error) => Err(CacheError::Io(error)),
    }
}

pub(crate) fn save(cache: &QuoteCache) -> Result<(), CacheError> {
    let path = cache_path()?;
    let contents = serde_json::to_vec_pretty(cache).map_err(CacheError::Encode)?;
    storage::write_atomically(&path, &contents).map_err(CacheError::Io)
}

fn recover_invalid_cache(path: &std::path::Path) -> Result<LoadResult, CacheError> {
    storage::quarantine_invalid_file(path).map_err(CacheError::Io)?;
    diagnostics::record(Event::CacheRecovered);
    Ok(LoadResult {
        cache: QuoteCache::default(),
        recovered: true,
    })
}

fn cache_path() -> Result<PathBuf, CacheError> {
    storage::data_dir()
        .map(|directory| directory.join(CACHE_FILE))
        .ok_or(CacheError::UnavailablePath)
}

#[derive(Debug)]
pub(crate) enum CacheError {
    UnavailablePath,
    Io(io::Error),
    Encode(serde_json::Error),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnavailablePath => write!(formatter, "could not determine the macOS cache path"),
            Self::Io(error) => write!(formatter, "cache I/O error: {error}"),
            Self::Encode(error) => write!(formatter, "cache serialization error: {error}"),
        }
    }
}

impl std::error::Error for CacheError {}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{QuoteCache, load_from_path};
    use crate::provider::Quote;

    #[test]
    fn serializes_current_and_previous_quotes() {
        let quote = Quote {
            symbol: "hf_XAU".to_owned(),
            display_name: "伦敦金（现货黄金）".to_owned(),
            price: Decimal::new(436_990, 2),
            percentage_change: Decimal::new(33, 2),
            previous_close: Decimal::new(435_541, 2),
            quote_date: "2026-09-09".to_owned(),
            quote_time: "10:33:00".to_owned(),
            fetched_at_unix_secs: 1_725_000_000,
            provider: "腾讯行情".to_owned(),
        };
        let cache = QuoteCache {
            current: Some(quote.clone()),
            previous: Some(quote),
        };

        let encoded = serde_json::to_vec(&cache).expect("cache must serialize");
        let decoded: QuoteCache = serde_json::from_slice(&encoded).expect("cache must deserialize");

        assert_eq!(decoded, cache);
    }

    #[test]
    fn recovers_from_invalid_cache_and_preserves_a_quarantine_copy() {
        let directory = std::env::temp_dir().join(format!(
            "gold-ticker-cache-recovery-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("quote-cache.json");
        std::fs::write(&path, "not json").unwrap();

        let loaded = load_from_path(&path).unwrap();

        assert!(loaded.recovered);
        assert_eq!(loaded.cache, QuoteCache::default());
        assert!(!path.exists());
        assert!(std::fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("quote-cache.json.invalid-")
        }));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
