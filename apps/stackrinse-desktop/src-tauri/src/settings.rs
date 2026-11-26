use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, PathBuf},
};
use tempfile::NamedTempFile;

const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    pub roots: Vec<PathBuf>,
    pub appearance: Appearance,
    pub excluded_names: Vec<String>,
    pub protected_projects: Vec<PathBuf>,
}

impl Settings {
    fn validate(&self) -> Result<()> {
        if self.roots.len() > 64 || self.protected_projects.len() > 16_384 {
            return Err("Too many saved locations.".into());
        }
        for path in self.roots.iter().chain(&self.protected_projects) {
            if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
                return Err("A saved location is invalid.".into());
            }
        }
        if self.excluded_names.len() > 512
            || self.excluded_names.iter().any(|s| {
                s.is_empty()
                    || s == "."
                    || s == ".."
                    || s.len() > 255
                    || s.contains(['/', '\\', '\0'])
                    || s.chars().any(char::is_control)
            })
        {
            return Err("Use single directory names for exclusions.".into());
        }
        Ok(())
    }
}

/// Only this fixed file within the OS application config directory is writable via IPC.
/// The sibling lock serializes read-modify-replace across app instances.
pub struct Store(pub PathBuf);

impl Store {
    fn file(&self) -> PathBuf {
        self.0.join("settings.json")
    }

    pub fn read(&self) -> Result<Settings> {
        let path = self.file();
        // Reject existing special entries (including FIFOs) before attempting a read.
        // The config directory is local, trusted user state, never a scan target.
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => return Err("The settings path must be a regular file.".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(error) => return Err(error.to_string()),
        }
        let file = File::open(path).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.take(MAX_SETTINGS_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err("The settings file exceeds the size limit.".into());
        }
        let settings: Settings = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Cannot read {}: {e}", self.file().display()))?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&self.0).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.0.join("settings.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock_exclusive()
            .map_err(|_| "Settings are busy. Try again.".to_string())?;
        let mut settings = self.read()?;
        change(&mut settings);
        settings.validate()?;
        let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err("The settings file exceeds the size limit.".into());
        }
        let mut tmp = NamedTempFile::new_in(&self.0).map_err(|e| e.to_string())?;
        tmp.write_all(&bytes).map_err(|e| e.to_string())?;
        tmp.as_file().sync_all().map_err(|e| e.to_string())?;
        tmp.persist(self.file()).map_err(|e| e.to_string())?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn execution_settings_are_rejected() {
        for key in [
            "execute",
            "experimentalCleanup",
            "quarantine",
            "restore",
            "purge",
            "permanentDeletion",
        ] {
            let input = serde_json::json!({(key):true});
            assert!(serde_json::from_value::<Settings>(input).is_err(), "{key}");
        }
    }
    #[test]
    fn storing_invalid_exclusions_preserves_the_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store(tmp.path().to_path_buf());
        store
            .update(|s| s.excluded_names = vec!["vendor".into()])
            .unwrap();
        assert!(store
            .update(|s| s.excluded_names.push("../code".into()))
            .is_err());
        assert_eq!(store.read().unwrap().excluded_names, ["vendor"]);
    }
    #[test]
    fn refuses_relative_roots_and_does_not_silently_reset_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store(tmp.path().to_path_buf());
        fs::write(store.file(), r#"{"roots":["../other"]}"#).unwrap();
        assert!(store.read().is_err());
        assert!(store.update(|_| {}).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn settings_are_private_and_existing_links_are_refused() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let tmp = tempfile::tempdir().unwrap();
        let store = Store(tmp.path().join("app"));
        store.update(|s| s.appearance = Appearance::Light).unwrap();
        assert_eq!(
            fs::metadata(store.file()).unwrap().permissions().mode() & 0o077,
            0
        );
        let moved = tmp.path().join("original");
        fs::rename(store.file(), &moved).unwrap();
        symlink(&moved, store.file()).unwrap();
        assert!(store.update(|s| s.appearance = Appearance::Dark).is_err());
    }
    #[test]
    fn corrupted_settings_are_preserved_and_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store(tmp.path().to_path_buf());
        fs::write(store.file(), b"{").unwrap();
        assert!(store.update(|s| s.appearance = Appearance::Light).is_err());
        assert_eq!(fs::read(store.file()).unwrap(), b"{");
    }
}
