//! UI-independent settings editing and application.
//!
//! `AppConfig` remains the source of truth for polling and server settings.
//! Theme, language, and always-on-top are kept here until they become part of
//! the persisted application configuration.

use std::{error::Error, fmt, time::Duration};

use crate::config::{AppConfig, ConfigError, PollingPreferences};

/// The visual theme selected by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Dark
    }
}

/// The language selected by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    SimplifiedChinese,
    TraditionalChinese,
}

impl Default for Language {
    fn default() -> Self {
        Self::English
    }
}

/// Settings that are local to this module because `AppConfig` does not yet
/// model them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSettings {
    theme: Theme,
    language: Language,
    always_on_top: bool,
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            language: Language::default(),
            always_on_top: false,
        }
    }
}

impl LocalSettings {
    pub const fn new(theme: Theme, language: Language, always_on_top: bool) -> Self {
        Self {
            theme,
            language,
            always_on_top,
        }
    }

    pub const fn theme(&self) -> Theme {
        self.theme
    }

    pub const fn language(&self) -> Language {
        self.language
    }

    pub const fn always_on_top(&self) -> bool {
        self.always_on_top
    }
}

/// A mutable copy of settings that can be edited without changing the
/// application's active configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsDraft {
    polling_interval: Duration,
    active_server: Option<usize>,
    local: LocalSettings,
}

impl SettingsDraft {
    /// Starts a draft from the current application and local settings.
    pub fn from_config(config: &AppConfig, local: LocalSettings) -> Self {
        Self {
            polling_interval: config.polling().interval(),
            active_server: config.active_server_index(),
            local,
        }
    }

    pub const fn polling_interval(&self) -> Duration {
        self.polling_interval
    }

    pub const fn active_server(&self) -> Option<usize> {
        self.active_server
    }

    pub const fn theme(&self) -> Theme {
        self.local.theme()
    }

    pub const fn language(&self) -> Language {
        self.local.language()
    }

    pub const fn always_on_top(&self) -> bool {
        self.local.always_on_top()
    }

    /// Sets the polling interval, validating it using the existing config API.
    pub fn set_polling_interval(&mut self, interval: Duration) -> Result<(), ConfigError> {
        PollingPreferences::new(interval)?;
        self.polling_interval = interval;
        Ok(())
    }

    pub fn set_active_server(&mut self, server: Option<usize>) {
        self.active_server = server;
    }

    pub const fn set_theme(&mut self, theme: Theme) {
        self.local.theme = theme;
    }

    pub const fn set_language(&mut self, language: Language) {
        self.local.language = language;
    }

    pub const fn set_always_on_top(&mut self, always_on_top: bool) {
        self.local.always_on_top = always_on_top;
    }

    /// Applies all draft values atomically. Neither target is changed when
    /// validation fails.
    pub fn apply(
        &self,
        config: &mut AppConfig,
        local: &mut LocalSettings,
    ) -> Result<(), SettingsError> {
        let polling = PollingPreferences::new(self.polling_interval)?;
        // Constructing a candidate validates the server index before mutation.
        AppConfig::new(config.servers().to_vec(), self.active_server, polling)?;

        config.set_active_server(self.active_server)?;
        config.set_polling(polling);
        *local = self.local;
        Ok(())
    }

    /// Discards edits and reloads the draft from the last applied values.
    pub fn cancel(&mut self, config: &AppConfig, local: LocalSettings) {
        *self = Self::from_config(config, local);
    }
}

/// Coordinates a committed settings snapshot and its editable draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsModel {
    applied: SettingsDraft,
    draft: SettingsDraft,
}

impl SettingsModel {
    pub fn new(config: &AppConfig, local: LocalSettings) -> Self {
        let applied = SettingsDraft::from_config(config, local);
        Self {
            applied,
            draft: applied,
        }
    }

    pub const fn draft(&self) -> &SettingsDraft {
        &self.draft
    }

    pub const fn draft_mut(&mut self) -> &mut SettingsDraft {
        &mut self.draft
    }

    /// Applies the draft and records it as the committed settings snapshot.
    pub fn apply(
        &mut self,
        config: &mut AppConfig,
        local: &mut LocalSettings,
    ) -> Result<(), SettingsError> {
        self.draft.apply(config, local)?;
        self.applied = self.draft;
        Ok(())
    }

    /// Restores the draft to the last successfully applied values.
    pub const fn cancel(&mut self) {
        self.draft = self.applied;
    }
}

/// Errors produced while applying a settings draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    Config(ConfigError),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
        }
    }
}

impl Error for SettingsError {}

impl From<ConfigError> for SettingsError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL, ServerProfile};

    fn config() -> AppConfig {
        let mut config = AppConfig::default();
        config.add_server(ServerProfile::new("one", "one.example.test").unwrap());
        config.add_server(ServerProfile::new("two", "two.example.test").unwrap());
        config
    }

    #[test]
    fn draft_edits_do_not_change_config_until_apply() {
        let config = config();
        let local = LocalSettings::default();
        let mut draft = SettingsDraft::from_config(&config, local);

        draft.set_polling_interval(Duration::from_secs(30)).unwrap();
        draft.set_active_server(Some(1));
        draft.set_theme(Theme::Light);
        draft.set_language(Language::TraditionalChinese);
        draft.set_always_on_top(true);

        assert_eq!(config.active_server_index(), Some(0));
        assert_eq!(config.polling().interval(), Duration::from_secs(60));
        assert_eq!(local, LocalSettings::default());
        assert_eq!(draft.active_server(), Some(1));
        assert_eq!(draft.theme(), Theme::Light);
    }

    #[test]
    fn apply_updates_config_and_local_settings() {
        let mut config = config();
        let mut local = LocalSettings::default();
        let mut model = SettingsModel::new(&config, local);
        model.draft_mut().set_active_server(Some(1));
        model
            .draft_mut()
            .set_polling_interval(Duration::from_secs(15))
            .unwrap();
        model.draft_mut().set_theme(Theme::System);
        model.draft_mut().set_language(Language::SimplifiedChinese);
        model.draft_mut().set_always_on_top(true);

        model.apply(&mut config, &mut local).unwrap();

        assert_eq!(config.active_server_index(), Some(1));
        assert_eq!(config.polling().interval(), Duration::from_secs(15));
        assert_eq!(
            local,
            LocalSettings::new(Theme::System, Language::SimplifiedChinese, true)
        );
    }

    #[test]
    fn cancel_discards_unapplied_edits() {
        let config = config();
        let local = LocalSettings::default();
        let mut model = SettingsModel::new(&config, local);
        model.draft_mut().set_active_server(Some(1));
        model.draft_mut().set_theme(Theme::Light);
        model.cancel();

        assert_eq!(model.draft().active_server(), Some(0));
        assert_eq!(model.draft().theme(), Theme::Dark);
    }

    #[test]
    fn invalid_polling_values_are_rejected_and_preserve_draft() {
        let config = config();
        let mut draft = SettingsDraft::from_config(&config, LocalSettings::default());
        let original = draft.polling_interval();

        assert!(
            draft
                .set_polling_interval(MIN_POLL_INTERVAL - Duration::from_secs(1))
                .is_err()
        );
        assert!(
            draft
                .set_polling_interval(MAX_POLL_INTERVAL + Duration::from_secs(1))
                .is_err()
        );
        assert_eq!(draft.polling_interval(), original);
    }
}
