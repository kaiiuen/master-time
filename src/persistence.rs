//! UI-independent lifecycle management for persisted [`AppConfig`] values.
//!
//! [`PersistenceManager`] keeps the in-memory configuration and its persisted
//! representation together. It can load an existing configuration, fall back
//! to [`AppConfig::default`] when the file does not exist, track mutations, and
//! optionally save each changed configuration immediately.

use crate::{AppConfig, StorageError, storage};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// Errors returned by the persistence lifecycle manager.
#[derive(Debug)]
pub enum PersistenceError {
    /// The configuration could not be loaded.
    Load(StorageError),
    /// The configuration could not be saved.
    Save(StorageError),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(source) => write!(formatter, "could not load configuration: {source}"),
            Self::Save(source) => write!(formatter, "could not save configuration: {source}"),
        }
    }
}

impl Error for PersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(source) | Self::Save(source) => Some(source),
        }
    }
}

/// Owns an application configuration and manages its persistence lifecycle.
#[derive(Debug)]
pub struct PersistenceManager {
    path: PathBuf,
    config: AppConfig,
    dirty: bool,
    save_on_change: bool,
}

/// Alias emphasizing that this type is the application's persistence layer.
pub type Persistence = PersistenceManager;

impl PersistenceManager {
    /// Creates a manager with `config` and no pending changes.
    pub fn new(path: impl Into<PathBuf>, config: AppConfig) -> Self {
        Self {
            path: path.into(),
            config,
            dirty: false,
            save_on_change: false,
        }
    }

    /// Loads a configuration from `path`.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, PersistenceError> {
        let path = path.into();
        let config = storage::load_config(&path).map_err(PersistenceError::Load)?;
        Ok(Self::new(path, config))
    }

    /// Loads a configuration, using [`AppConfig::default`] only when `path`
    /// does not exist.
    pub fn load_or_default(path: impl Into<PathBuf>) -> Result<Self, PersistenceError> {
        let path = path.into();
        let config = match storage::load_config(&path) {
            Ok(config) => config,
            Err(StorageError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                AppConfig::default()
            }
            Err(error) => return Err(PersistenceError::Load(error)),
        };
        Ok(Self::new(path, config))
    }

    /// Loads a configuration and enables or disables save-on-change behavior.
    pub fn load_or_default_with_save_on_change(
        path: impl Into<PathBuf>,
        save_on_change: bool,
    ) -> Result<Self, PersistenceError> {
        let mut manager = Self::load_or_default(path)?;
        manager.save_on_change = save_on_change;
        Ok(manager)
    }

    /// Returns the file used by this manager.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the current in-memory configuration.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Returns whether the in-memory configuration differs from the last save
    /// or load.
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns whether changes are saved automatically.
    pub const fn saves_on_change(&self) -> bool {
        self.save_on_change
    }

    /// Enables or disables saving after a changed mutation.
    pub fn set_save_on_change(&mut self, enabled: bool) {
        self.save_on_change = enabled;
    }

    /// Replaces the configuration and saves it immediately when configured to
    /// do so. A failed automatic save leaves the manager dirty.
    pub fn set_config(&mut self, config: AppConfig) -> Result<(), PersistenceError> {
        if self.config == config {
            return Ok(());
        }
        self.config = config;
        self.dirty = true;
        self.save_if_configured()
    }

    /// Gives a mutation closure access to the configuration. The dirty flag is
    /// set only when the closure actually changes the value.
    pub fn update<F>(&mut self, update: F) -> Result<(), PersistenceError>
    where
        F: FnOnce(&mut AppConfig),
    {
        let before = self.config.clone();
        update(&mut self.config);
        if self.config != before {
            self.dirty = true;
            self.save_if_configured()?;
        }
        Ok(())
    }

    /// Saves the current configuration, even when it is not dirty.
    pub fn save(&mut self) -> Result<(), PersistenceError> {
        storage::save_config(&self.path, &self.config).map_err(PersistenceError::Save)?;
        self.dirty = false;
        Ok(())
    }

    /// Saves only when there are pending changes.
    pub fn save_if_dirty(&mut self) -> Result<bool, PersistenceError> {
        if self.dirty {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn save_if_configured(&mut self) -> Result<(), PersistenceError> {
        if self.save_on_change {
            self.save()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PollingPreferences, ServerProfile};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDirectory {
        path: PathBuf,
    }

    impl TempDirectory {
        fn new() -> Self {
            let nonce = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before the Unix epoch")
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("master-time-persistence-{timestamp}-{nonce}"));
            fs::create_dir(&path).expect("create temporary directory");
            Self { path }
        }

        fn file(&self) -> PathBuf {
            self.path.join("config")
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn missing_file_loads_default_and_starts_clean() {
        let directory = TempDirectory::new();
        let manager = PersistenceManager::load_or_default(directory.file()).unwrap();

        assert_eq!(manager.config(), &AppConfig::default());
        assert!(!manager.is_dirty());
    }

    #[test]
    fn explicit_save_clears_dirty_state_and_round_trips() {
        let directory = TempDirectory::new();
        let file = directory.file();
        let mut manager = PersistenceManager::load_or_default(&file).unwrap();
        let server = ServerProfile::new("Office", "time.example.com").unwrap();

        manager
            .update(|config| {
                config.add_server(server);
                config.set_polling(PollingPreferences::new(Duration::from_secs(120)).unwrap());
            })
            .unwrap();
        assert!(manager.is_dirty());
        assert!(manager.save_if_dirty().unwrap());
        assert!(!manager.is_dirty());

        let loaded = PersistenceManager::load(&file).unwrap();
        assert_eq!(loaded.config(), manager.config());
    }

    #[test]
    fn save_on_change_persists_mutations_immediately() {
        let directory = TempDirectory::new();
        let file = directory.file();
        let mut manager =
            PersistenceManager::load_or_default_with_save_on_change(&file, true).unwrap();

        manager
            .update(|config| {
                config.set_polling(PollingPreferences::new(Duration::from_secs(90)).unwrap())
            })
            .unwrap();

        assert!(!manager.is_dirty());
        assert_eq!(
            PersistenceManager::load(&file).unwrap().config(),
            manager.config()
        );
    }

    #[test]
    fn non_missing_load_errors_are_typed() {
        let directory = TempDirectory::new();
        let error = PersistenceManager::load_or_default(directory.path.clone()).unwrap_err();

        assert!(matches!(
            error,
            PersistenceError::Load(StorageError::Io { .. })
        ));
    }
}
