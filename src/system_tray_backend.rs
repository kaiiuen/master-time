//! Platform boundary for connecting the system-tray command model to a tray UI.
//!
//! This module intentionally does not claim to create a native tray. The
//! Windows side provides the lifecycle and event boundary needed by a future
//! Win32 implementation without introducing an unsafe API or a dependency.
//! Other platforms report that the boundary is unsupported and do nothing.

use crate::system_tray::{MENU_ITEMS, SystemTrayState, TrayAction, TrayCommand, TrayEvent};

/// Stable identifiers a native menu may associate with [`TrayCommand`]s.
///
/// Keeping these IDs here makes the translation from a platform callback to
/// the model explicit instead of relying on menu position or labels.
pub const MENU_COMMAND_IDS: [(u16, TrayCommand); 5] = [
    (100, TrayCommand::Show),
    (101, TrayCommand::Hide),
    (102, TrayCommand::StartPolling),
    (103, TrayCommand::StopPolling),
    (104, TrayCommand::Quit),
];

/// Whether this build has a platform adapter boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayBackendAvailability {
    /// Windows can host the future native implementation through this boundary.
    WindowsBoundary,
    /// No native implementation is provided for this target.
    Unsupported,
}

/// Lifecycle of the adapter boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayBackendLifecycle {
    Created,
    Running,
    Stopped,
}

/// Errors are explicit so callers cannot mistake a no-op for a native tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayBackendError {
    UnsupportedPlatform,
    NotRunning,
    AlreadyStopped,
    UnknownMenuId(u16),
}

impl TrayBackendError {
    pub const fn is_unsupported(self) -> bool {
        matches!(self, Self::UnsupportedPlatform)
    }
}

/// Platform-neutral state owned by the tray adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemTrayBackend {
    model: SystemTrayState,
    lifecycle: TrayBackendLifecycle,
}

impl SystemTrayBackend {
    pub const fn new(model: SystemTrayState) -> Self {
        Self {
            model,
            lifecycle: TrayBackendLifecycle::Created,
        }
    }

    pub const fn availability() -> TrayBackendAvailability {
        platform::availability()
    }

    pub const fn lifecycle(&self) -> TrayBackendLifecycle {
        self.lifecycle
    }

    pub const fn model(&self) -> &SystemTrayState {
        &self.model
    }

    /// Starts the boundary. On Windows this only records readiness; it does
    /// not create a native icon or menu because no native API is wired here.
    pub fn initialize(&mut self) -> Result<(), TrayBackendError> {
        platform::initialize(&mut self.lifecycle)
    }

    /// Translates a native menu identifier and forwards it to the command
    /// model. Model state changes only occur after successful initialization.
    pub fn dispatch_menu_id(&mut self, menu_id: u16) -> Result<TrayAction, TrayBackendError> {
        let command =
            command_for_menu_id(menu_id).ok_or(TrayBackendError::UnknownMenuId(menu_id))?;
        platform::dispatch(
            &mut self.lifecycle,
            &mut self.model,
            TrayEvent::Command(command),
        )
    }

    /// Stops the boundary. Repeated shutdown is a harmless no-op.
    pub fn shutdown(&mut self) -> Result<(), TrayBackendError> {
        platform::shutdown(&mut self.lifecycle)
    }
}

/// Converts a platform menu callback ID into the existing command model.
pub fn command_for_menu_id(menu_id: u16) -> Option<TrayCommand> {
    MENU_COMMAND_IDS
        .iter()
        .find_map(|(id, command)| (*id == menu_id).then_some(*command))
}

/// Returns the menu entries a native implementation should render.
pub const fn menu_items() -> &'static [crate::system_tray::TrayMenuItem; 5] {
    &MENU_ITEMS
}

#[cfg(windows)]
mod platform {
    use super::*;

    pub const fn availability() -> TrayBackendAvailability {
        TrayBackendAvailability::WindowsBoundary
    }

    pub fn initialize(lifecycle: &mut TrayBackendLifecycle) -> Result<(), TrayBackendError> {
        match lifecycle {
            TrayBackendLifecycle::Created => {
                *lifecycle = TrayBackendLifecycle::Running;
                Ok(())
            }
            TrayBackendLifecycle::Running => Ok(()),
            TrayBackendLifecycle::Stopped => Err(TrayBackendError::AlreadyStopped),
        }
    }

    pub fn dispatch(
        lifecycle: &mut TrayBackendLifecycle,
        model: &mut SystemTrayState,
        event: TrayEvent,
    ) -> Result<TrayAction, TrayBackendError> {
        if *lifecycle != TrayBackendLifecycle::Running {
            return Err(TrayBackendError::NotRunning);
        }
        Ok(model.handle_event(event))
    }

    pub fn shutdown(lifecycle: &mut TrayBackendLifecycle) -> Result<(), TrayBackendError> {
        match lifecycle {
            TrayBackendLifecycle::Created | TrayBackendLifecycle::Running => {
                *lifecycle = TrayBackendLifecycle::Stopped;
                Ok(())
            }
            TrayBackendLifecycle::Stopped => Ok(()),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub const fn availability() -> TrayBackendAvailability {
        TrayBackendAvailability::Unsupported
    }

    pub fn initialize(_lifecycle: &mut TrayBackendLifecycle) -> Result<(), TrayBackendError> {
        Err(TrayBackendError::UnsupportedPlatform)
    }

    pub fn dispatch(
        _lifecycle: &mut TrayBackendLifecycle,
        _model: &mut SystemTrayState,
        _event: TrayEvent,
    ) -> Result<TrayAction, TrayBackendError> {
        Err(TrayBackendError::UnsupportedPlatform)
    }

    pub fn shutdown(_lifecycle: &mut TrayBackendLifecycle) -> Result<(), TrayBackendError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_every_model_command_without_using_labels() {
        let commands = MENU_COMMAND_IDS
            .iter()
            .map(|(id, _)| command_for_menu_id(*id))
            .collect::<Option<Vec<_>>>();
        assert_eq!(
            commands,
            Some(
                MENU_COMMAND_IDS
                    .iter()
                    .map(|(_, command)| *command)
                    .collect()
            )
        );
        assert_eq!(command_for_menu_id(999), None);
    }

    #[test]
    fn new_backend_has_explicit_created_lifecycle() {
        let backend = SystemTrayBackend::new(SystemTrayState::new());
        assert_eq!(backend.lifecycle(), TrayBackendLifecycle::Created);
        #[cfg(windows)]
        assert_eq!(
            SystemTrayBackend::availability(),
            TrayBackendAvailability::WindowsBoundary
        );
        #[cfg(not(windows))]
        assert_eq!(
            SystemTrayBackend::availability(),
            TrayBackendAvailability::Unsupported
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_target_does_not_mutate_state_or_lifecycle() {
        let mut backend = SystemTrayBackend::new(SystemTrayState::new());
        assert_eq!(
            backend.initialize(),
            Err(TrayBackendError::UnsupportedPlatform)
        );
        assert_eq!(backend.lifecycle(), TrayBackendLifecycle::Created);
        assert_eq!(
            backend.dispatch_menu_id(101),
            Err(TrayBackendError::UnsupportedPlatform)
        );
        assert!(backend.model().window_visible());
        assert_eq!(backend.shutdown(), Ok(()));
        assert_eq!(backend.lifecycle(), TrayBackendLifecycle::Created);
    }
}
