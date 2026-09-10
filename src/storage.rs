use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;

const APPLICATION_QUALIFIER: &str = "com";
const ORGANIZATION: &str = "GoldTicker";
const APPLICATION: &str = "GoldTicker";
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn data_dir() -> Option<PathBuf> {
    ProjectDirs::from(APPLICATION_QUALIFIER, ORGANIZATION, APPLICATION)
        .map(|project_dirs| project_dirs.data_local_dir().to_owned())
}

pub(crate) fn write_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let directory = path
        .parent()
        .expect("persistent path must have a parent directory");
    fs::create_dir_all(directory)?;
    cleanup_stale_temporary_files(path)?;

    let temporary_path = temporary_path(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary_path, path)?;
        let _ = File::open(directory).and_then(|directory| directory.sync_all());
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

pub(crate) fn quarantine_invalid_file(path: &Path) -> io::Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let quarantined_path = invalid_path(path);
    fs::rename(path, &quarantined_path)?;
    Ok(Some(quarantined_path))
}

pub(crate) fn cleanup_stale_temporary_files(path: &Path) -> io::Result<()> {
    let directory = path
        .parent()
        .expect("persistent path must have a parent directory");
    let file_name = path
        .file_name()
        .expect("persistent path must have a file name")
        .to_string_lossy();
    let prefix = format!(".{file_name}.tmp-");
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    for entry in entries {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let directory = path
        .parent()
        .expect("persistent path must have a parent directory");
    let file_name = path
        .file_name()
        .expect("persistent path must have a file name")
        .to_string_lossy();
    directory.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn invalid_path(path: &Path) -> PathBuf {
    let directory = path
        .parent()
        .expect("persistent path must have a parent directory");
    let file_name = path
        .file_name()
        .expect("persistent path must have a file name")
        .to_string_lossy();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    directory.join(format!(
        "{file_name}.invalid-{timestamp}-{}",
        UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{cleanup_stale_temporary_files, quarantine_invalid_file, write_atomically};

    fn temporary_directory() -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "gold-ticker-storage-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn writes_atomically_and_cleans_its_temporary_files() {
        let directory = temporary_directory();
        let path = directory.join("settings.toml");
        fs::write(directory.join(".settings.toml.tmp-old"), "partial").unwrap();

        write_atomically(&path, b"complete").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "complete");
        assert!(fs::read_dir(&directory).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-")
        }));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn quarantines_invalid_files_without_overwriting_them() {
        let directory = temporary_directory();
        let path = directory.join("quote-cache.json");
        fs::write(&path, "invalid").unwrap();

        let quarantined = quarantine_invalid_file(&path).unwrap().unwrap();

        assert!(!path.exists());
        assert_eq!(fs::read_to_string(quarantined).unwrap(), "invalid");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cleanup_ignores_unrelated_files() {
        let directory = temporary_directory();
        let path = directory.join("settings.toml");
        let unrelated = directory.join(".quote-cache.json.tmp-old");
        fs::write(&unrelated, "keep").unwrap();
        fs::write(directory.join(".settings.toml.tmp-old"), "remove").unwrap();

        cleanup_stale_temporary_files(&path).unwrap();

        assert!(unrelated.exists());
        assert!(!directory.join(".settings.toml.tmp-old").exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
