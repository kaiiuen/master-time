//! UI-independent state and command handling for a system tray.
//!
//! This module deliberately contains no windowing or platform API calls. A
//! platform adapter can render [`MENU_ITEMS`] and apply the [`TrayAction`]
//! returned by [`SystemTrayState::handle_event`].

/// The labels used by the system tray menu.
pub const SHOW_LABEL: &str = "Show";
pub const HIDE_LABEL: &str = "Hide";
pub const START_POLLING_LABEL: &str = "Start polling";
pub const STOP_POLLING_LABEL: &str = "Stop polling";
pub const QUIT_LABEL: &str = "Quit";

/// Commands exposed by the system tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Hide,
    StartPolling,
    StopPolling,
    Quit,
}

/// An event received from a tray integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// A menu item was selected.
    Command(TrayCommand),
}

/// A menu item that a platform-specific tray integration can render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayMenuItem {
    pub label: &'static str,
    pub command: TrayCommand,
}

/// The complete, platform-neutral tray menu.
pub const MENU_ITEMS: [TrayMenuItem; 5] = [
    TrayMenuItem {
        label: SHOW_LABEL,
        command: TrayCommand::Show,
    },
    TrayMenuItem {
        label: HIDE_LABEL,
        command: TrayCommand::Hide,
    },
    TrayMenuItem {
        label: START_POLLING_LABEL,
        command: TrayCommand::StartPolling,
    },
    TrayMenuItem {
        label: STOP_POLLING_LABEL,
        command: TrayCommand::StopPolling,
    },
    TrayMenuItem {
        label: QUIT_LABEL,
        command: TrayCommand::Quit,
    },
];

/// The state changes a platform adapter should apply after handling a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// The command was already reflected in the state.
    Noop,
    Show,
    Hide,
    StartPolling,
    StopPolling,
    Quit,
}

/// UI-independent state represented by the system tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemTrayState {
    window_visible: bool,
    polling: bool,
    quit_requested: bool,
}

impl Default for SystemTrayState {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemTrayState {
    /// Creates the normal initial state: the window is visible and polling is
    /// stopped.
    pub const fn new() -> Self {
        Self {
            window_visible: true,
            polling: false,
            quit_requested: false,
        }
    }

    /// Creates a state with explicitly supplied visibility and polling values.
    pub const fn with_state(window_visible: bool, polling: bool) -> Self {
        Self {
            window_visible,
            polling,
            quit_requested: false,
        }
    }

    pub const fn window_visible(&self) -> bool {
        self.window_visible
    }

    pub const fn polling(&self) -> bool {
        self.polling
    }

    pub const fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// Handles an event and returns the side effect for the owning application.
    ///
    /// Repeating a command that is already satisfied returns [`TrayAction::Noop`].
    /// Quit is latched so subsequent events cannot accidentally clear it.
    pub fn handle_event(&mut self, event: TrayEvent) -> TrayAction {
        match event {
            TrayEvent::Command(command) => self.handle_command(command),
        }
    }

    /// Handles a tray command and updates this model's state.
    pub fn handle_command(&mut self, command: TrayCommand) -> TrayAction {
        match command {
            TrayCommand::Show if !self.window_visible => {
                self.window_visible = true;
                TrayAction::Show
            }
            TrayCommand::Show => TrayAction::Noop,
            TrayCommand::Hide if self.window_visible => {
                self.window_visible = false;
                TrayAction::Hide
            }
            TrayCommand::Hide => TrayAction::Noop,
            TrayCommand::StartPolling if !self.polling => {
                self.polling = true;
                TrayAction::StartPolling
            }
            TrayCommand::StartPolling => TrayAction::Noop,
            TrayCommand::StopPolling if self.polling => {
                self.polling = false;
                TrayAction::StopPolling
            }
            TrayCommand::StopPolling => TrayAction::Noop,
            TrayCommand::Quit if !self.quit_requested => {
                self.quit_requested = true;
                TrayAction::Quit
            }
            TrayCommand::Quit => TrayAction::Noop,
        }
    }
}

/// Returns the menu items in their display order.
pub const fn menu_items() -> &'static [TrayMenuItem; 5] {
    &MENU_ITEMS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_visible_and_not_polling() {
        let state = SystemTrayState::default();

        assert!(state.window_visible());
        assert!(!state.polling());
        assert!(!state.quit_requested());
    }

    #[test]
    fn commands_update_state_and_return_actions() {
        let mut state = SystemTrayState::new();

        assert_eq!(state.handle_command(TrayCommand::Hide), TrayAction::Hide);
        assert!(!state.window_visible());
        assert_eq!(state.handle_command(TrayCommand::Show), TrayAction::Show);
        assert!(state.window_visible());
        assert_eq!(
            state.handle_event(TrayEvent::Command(TrayCommand::StartPolling)),
            TrayAction::StartPolling
        );
        assert!(state.polling());
        assert_eq!(
            state.handle_command(TrayCommand::StopPolling),
            TrayAction::StopPolling
        );
        assert!(!state.polling());
    }

    #[test]
    fn repeated_commands_are_noops() {
        let mut state = SystemTrayState::new();

        assert_eq!(state.handle_command(TrayCommand::Show), TrayAction::Noop);
        assert_eq!(
            state.handle_command(TrayCommand::StopPolling),
            TrayAction::Noop
        );
        assert_eq!(state.handle_command(TrayCommand::Hide), TrayAction::Hide);
        assert_eq!(state.handle_command(TrayCommand::Hide), TrayAction::Noop);
        assert_eq!(
            state.handle_command(TrayCommand::StartPolling),
            TrayAction::StartPolling
        );
        assert_eq!(
            state.handle_command(TrayCommand::StartPolling),
            TrayAction::Noop
        );
    }

    #[test]
    fn quit_is_latched_and_does_not_change_other_state() {
        let mut state = SystemTrayState::with_state(false, true);

        assert_eq!(state.handle_command(TrayCommand::Quit), TrayAction::Quit);
        assert!(state.quit_requested());
        assert!(!state.window_visible());
        assert!(state.polling());
        assert_eq!(state.handle_command(TrayCommand::Quit), TrayAction::Noop);
    }

    #[test]
    fn menu_contains_expected_labels_and_commands() {
        let commands = menu_items()
            .iter()
            .map(|item| item.command)
            .collect::<Vec<_>>();
        let labels = menu_items()
            .iter()
            .map(|item| item.label)
            .collect::<Vec<_>>();

        assert_eq!(
            commands,
            vec![
                TrayCommand::Show,
                TrayCommand::Hide,
                TrayCommand::StartPolling,
                TrayCommand::StopPolling,
                TrayCommand::Quit,
            ]
        );
        assert_eq!(
            labels,
            vec![
                SHOW_LABEL,
                HIDE_LABEL,
                START_POLLING_LABEL,
                STOP_POLLING_LABEL,
                QUIT_LABEL
            ]
        );
    }
}
