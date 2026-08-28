//! UI-independent management of configured server profiles.
//!
//! [`ServerManager`] coordinates the validated configuration APIs with the
//! application state. It deliberately contains no presentation or persistence
//! concerns.

use std::fmt;

use crate::config::{AppConfig, ConfigError, ServerProfile};
use crate::servers::ServerCatalog;
use crate::state::ApplicationState;

/// Errors returned by server management operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerManagerError {
    /// The supplied profile failed configuration validation.
    InvalidProfile(ConfigError),
    /// A profile with the same name and hostname is already configured.
    DuplicateProfile,
    /// The requested profile index does not exist.
    ProfileNotFound { index: usize },
    /// The active profile must be changed before it can be removed.
    ActiveProfileRemoval,
}

impl fmt::Display for ServerManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfile(error) => write!(formatter, "invalid server profile: {error}"),
            Self::DuplicateProfile => formatter.write_str("server profile already exists"),
            Self::ProfileNotFound { index } => {
                write!(formatter, "server profile {index} not found")
            }
            Self::ActiveProfileRemoval => {
                formatter.write_str("select another server before removing the active server")
            }
        }
    }
}

impl std::error::Error for ServerManagerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidProfile(error) => Some(error),
            _ => None,
        }
    }
}

/// Manages configured profiles and the currently selected profile.
#[derive(Debug)]
pub struct ServerManager {
    state: ApplicationState,
    catalog: ServerCatalog,
}

impl ServerManager {
    /// Creates a manager over application state and a server catalog.
    pub fn new(state: ApplicationState, catalog: ServerCatalog) -> Self {
        Self { state, catalog }
    }

    /// Returns the catalog used to offer built-in server profiles.
    pub const fn catalog(&self) -> &ServerCatalog {
        &self.catalog
    }

    /// Returns all configured profiles in their stable configuration order.
    pub fn profiles(&self) -> &[ServerProfile] {
        self.state.config().servers()
    }

    /// Alias for callers that describe the operation as listing profiles.
    pub fn list(&self) -> &[ServerProfile] {
        self.profiles()
    }

    /// Returns the selected profile index, if one is selected.
    pub const fn selected_index(&self) -> Option<usize> {
        self.state.config().active_server_index()
    }

    /// Returns the selected profile, if one is selected.
    pub fn selected(&self) -> Option<&ServerProfile> {
        self.state.active_server()
    }

    /// Returns the underlying application state as a read-only view.
    pub const fn state(&self) -> &ApplicationState {
        &self.state
    }

    /// Adds a validated profile and returns its new index.
    pub fn add_profile(&mut self, profile: ServerProfile) -> Result<usize, ServerManagerError> {
        self.ensure_unique(&profile, None)?;
        let mut config = self.state.config().clone();
        config.add_server(profile);
        let index = config.servers().len() - 1;
        self.replace_state(config);
        Ok(index)
    }

    /// Validates and adds a profile from user-provided text.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        hostname: impl Into<String>,
    ) -> Result<usize, ServerManagerError> {
        let profile =
            ServerProfile::new(name, hostname).map_err(ServerManagerError::InvalidProfile)?;
        self.add_profile(profile)
    }

    /// Edits an existing profile and returns the updated profile index.
    pub fn edit_profile(
        &mut self,
        index: usize,
        profile: ServerProfile,
    ) -> Result<(), ServerManagerError> {
        self.require_index(index)?;
        self.ensure_unique(&profile, Some(index))?;
        let mut profiles = self.profiles().to_vec();
        profiles[index] = profile;
        let config = self.rebuild_config(profiles, self.selected_index());
        self.replace_state(config.expect("existing configuration must remain valid"));
        Ok(())
    }

    /// Validates and edits an existing profile from user-provided text.
    pub fn edit(
        &mut self,
        index: usize,
        name: impl Into<String>,
        hostname: impl Into<String>,
    ) -> Result<(), ServerManagerError> {
        let profile =
            ServerProfile::new(name, hostname).map_err(ServerManagerError::InvalidProfile)?;
        self.edit_profile(index, profile)
    }

    /// Removes a non-active profile.
    pub fn remove_profile(&mut self, index: usize) -> Result<ServerProfile, ServerManagerError> {
        self.require_index(index)?;
        if self.selected_index() == Some(index) {
            return Err(ServerManagerError::ActiveProfileRemoval);
        }

        let mut profiles = self.profiles().to_vec();
        let removed = profiles.remove(index);
        let active = self.selected_index().map(|selected| {
            if selected > index {
                selected - 1
            } else {
                selected
            }
        });
        let config = self.rebuild_config(profiles, active);
        self.replace_state(config.expect("existing configuration must remain valid"));
        Ok(removed)
    }

    /// Removes a non-active profile.
    pub fn remove(&mut self, index: usize) -> Result<ServerProfile, ServerManagerError> {
        self.remove_profile(index)
    }

    /// Selects a profile, or clears the selection with `None`.
    pub fn select(&mut self, index: Option<usize>) -> Result<(), ServerManagerError> {
        self.state
            .set_active_server(index)
            .map_err(|error| match error {
                ConfigError::ActiveServerOutOfBounds { index, .. } => {
                    ServerManagerError::ProfileNotFound { index }
                }
                other => ServerManagerError::InvalidProfile(other),
            })
    }

    /// Alias for callers that use the profile terminology for selection.
    pub fn select_profile(&mut self, index: Option<usize>) -> Result<(), ServerManagerError> {
        self.select(index)
    }

    fn require_index(&self, index: usize) -> Result<(), ServerManagerError> {
        if index < self.profiles().len() {
            Ok(())
        } else {
            Err(ServerManagerError::ProfileNotFound { index })
        }
    }

    fn ensure_unique(
        &self,
        profile: &ServerProfile,
        excluded_index: Option<usize>,
    ) -> Result<(), ServerManagerError> {
        if self
            .profiles()
            .iter()
            .enumerate()
            .any(|(index, existing)| Some(index) != excluded_index && existing == profile)
        {
            Err(ServerManagerError::DuplicateProfile)
        } else {
            Ok(())
        }
    }

    fn rebuild_config(
        &self,
        profiles: Vec<ServerProfile>,
        active: Option<usize>,
    ) -> Result<AppConfig, ConfigError> {
        AppConfig::new(profiles, active, self.state.config().polling())
    }

    fn replace_state(&mut self, config: AppConfig) {
        let capacity = self.state.history().capacity();
        self.state = ApplicationState::new(config, capacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    fn manager() -> ServerManager {
        ServerManager::new(
            ApplicationState::new(AppConfig::default(), 8),
            ServerCatalog::built_in(),
        )
    }

    fn profile(name: &str, host: &str) -> ServerProfile {
        ServerProfile::new(name, host).unwrap()
    }

    #[test]
    fn add_selects_first_profile_and_lists_profiles() {
        let mut manager = manager();
        assert_eq!(manager.add_profile(profile("One", "one.example")), Ok(0));
        assert_eq!(manager.add_profile(profile("Two", "two.example")), Ok(1));
        assert_eq!(manager.list().len(), 2);
        assert_eq!(manager.selected_index(), Some(0));
    }

    #[test]
    fn edit_changes_profile() {
        let mut manager = manager();
        manager.add_profile(profile("One", "one.example")).unwrap();
        manager.edit(0, "Updated", "updated.example").unwrap();
        assert_eq!(manager.profiles()[0], profile("Updated", "updated.example"));
    }

    #[test]
    fn remove_rejects_active_and_removes_other_profile() {
        let mut manager = manager();
        manager.add_profile(profile("One", "one.example")).unwrap();
        manager.add_profile(profile("Two", "two.example")).unwrap();
        assert_eq!(
            manager.remove_profile(0),
            Err(ServerManagerError::ActiveProfileRemoval)
        );
        assert_eq!(
            manager.remove_profile(1).unwrap(),
            profile("Two", "two.example")
        );
        assert_eq!(manager.profiles().len(), 1);
    }

    #[test]
    fn selection_changes_active_profile() {
        let mut manager = manager();
        manager.add_profile(profile("One", "one.example")).unwrap();
        manager.add_profile(profile("Two", "two.example")).unwrap();
        manager.select(Some(1)).unwrap();
        assert_eq!(manager.selected().unwrap().name(), "Two");
        manager.select(None).unwrap();
        assert!(manager.selected().is_none());
    }

    #[test]
    fn duplicate_profiles_are_rejected() {
        let mut manager = manager();
        manager.add_profile(profile("One", "one.example")).unwrap();
        assert_eq!(
            manager.add_profile(profile("One", "one.example")),
            Err(ServerManagerError::DuplicateProfile)
        );
    }

    #[test]
    fn invalid_profiles_are_rejected_without_mutation() {
        let mut manager = manager();
        assert!(matches!(
            manager.add("", "one.example"),
            Err(ServerManagerError::InvalidProfile(
                ConfigError::EmptyServerName
            ))
        ));
        assert!(matches!(
            manager.add("One", "bad host"),
            Err(ServerManagerError::InvalidProfile(
                ConfigError::InvalidHostname
            ))
        ));
        assert!(manager.profiles().is_empty());
    }
}
