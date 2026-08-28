//! UI-independent accessibility preferences and keyboard navigation.
//!
//! The types in this module deliberately do not depend on `egui` or any other
//! presentation toolkit. A UI can map its widgets to [`FocusTarget`] values,
//! pass keys to [`KeyboardNavigation`], and apply the resulting state change.

/// User preferences that affect accessibility-related presentation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibilityPreferences {
    high_contrast: bool,
    reduced_motion: bool,
}

impl Default for AccessibilityPreferences {
    fn default() -> Self {
        Self {
            high_contrast: false,
            reduced_motion: false,
        }
    }
}

impl AccessibilityPreferences {
    /// Creates preferences with both accessibility options disabled.
    pub const fn new() -> Self {
        Self {
            high_contrast: false,
            reduced_motion: false,
        }
    }

    /// Creates preferences from their two independent options.
    pub const fn with_options(high_contrast: bool, reduced_motion: bool) -> Self {
        Self {
            high_contrast,
            reduced_motion,
        }
    }

    pub const fn high_contrast(&self) -> bool {
        self.high_contrast
    }

    pub const fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    pub const fn set_high_contrast(&mut self, enabled: bool) {
        self.high_contrast = enabled;
    }

    pub const fn set_reduced_motion(&mut self, enabled: bool) {
        self.reduced_motion = enabled;
    }

    pub const fn toggle_high_contrast(&mut self) {
        self.high_contrast = !self.high_contrast;
    }

    pub const fn toggle_reduced_motion(&mut self) {
        self.reduced_motion = !self.reduced_motion;
    }
}

/// A focusable item in the application chrome.
///
/// Indices are stable identifiers supplied by the caller. The navigation model
/// does not know anything about labels or widget implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Tab(usize),
    Action(usize),
}

/// Keys understood by [`KeyboardNavigation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationKey {
    Tab,
    ShiftTab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Enter,
    Space,
}

/// The result of processing a navigation key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationResult {
    /// Focus moved to this item.
    Focused(FocusTarget),
    /// The focused item was activated.
    Activated(FocusTarget),
    /// The key has no effect for the current model state.
    Ignored,
}

/// State machine for keyboard focus and activation of tabs and actions.
///
/// Focus order is always all tabs in declaration order followed by all actions
/// in declaration order. Tab and arrow navigation wraps within that complete
/// order; `Home` and `End` select its first and last item. Activation changes
/// the active tab when a tab is activated and reports the target for actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardNavigation {
    focus_order: Vec<FocusTarget>,
    focused: Option<usize>,
    active_tab: Option<usize>,
}

impl KeyboardNavigation {
    /// Builds a navigation model with `tab_count` tabs and `action_count`
    /// actions. The initial focus is the first item, if any.
    pub fn new(tab_count: usize, action_count: usize) -> Self {
        let mut focus_order = Vec::with_capacity(tab_count.saturating_add(action_count));
        focus_order.extend((0..tab_count).map(FocusTarget::Tab));
        focus_order.extend((0..action_count).map(FocusTarget::Action));

        Self {
            focused: (!focus_order.is_empty()).then_some(0),
            focus_order,
            active_tab: (tab_count > 0).then_some(0),
        }
    }

    /// Returns the complete, stable focus order.
    pub fn focus_order(&self) -> &[FocusTarget] {
        &self.focus_order
    }

    pub fn focused(&self) -> Option<FocusTarget> {
        match self.focused {
            Some(index) => match self.focus_order.get(index) {
                Some(FocusTarget::Tab(tab)) => Some(FocusTarget::Tab(*tab)),
                Some(FocusTarget::Action(action)) => Some(FocusTarget::Action(*action)),
                None => None,
            },
            None => None,
        }
    }

    pub const fn active_tab(&self) -> Option<usize> {
        self.active_tab
    }

    /// Processes a key and returns the focus or activation event it produced.
    pub fn handle_key(&mut self, key: NavigationKey) -> NavigationResult {
        if self.focus_order.is_empty() {
            return NavigationResult::Ignored;
        }

        match key {
            NavigationKey::Tab | NavigationKey::Right | NavigationKey::Down => self.move_by(1),
            NavigationKey::ShiftTab | NavigationKey::Left | NavigationKey::Up => self.move_by(-1),
            NavigationKey::Home => self.set_focus(0),
            NavigationKey::End => self.set_focus(self.focus_order.len() - 1),
            NavigationKey::Enter | NavigationKey::Space => self.activate(),
        }
    }

    /// Sets focus to a target in the model's order, returning whether it exists.
    pub fn focus(&mut self, target: FocusTarget) -> bool {
        match self.focus_order.iter().position(|item| *item == target) {
            Some(index) => {
                self.focused = Some(index);
                true
            }
            None => false,
        }
    }

    fn move_by(&mut self, amount: isize) -> NavigationResult {
        let current = self.focused.unwrap_or(0) as isize;
        let length = self.focus_order.len() as isize;
        let next = (current + amount).rem_euclid(length) as usize;
        self.set_focus(next)
    }

    fn set_focus(&mut self, index: usize) -> NavigationResult {
        self.focused = Some(index);
        NavigationResult::Focused(self.focus_order[index])
    }

    fn activate(&mut self) -> NavigationResult {
        let Some(index) = self.focused else {
            return NavigationResult::Ignored;
        };
        let target = self.focus_order[index];
        if let FocusTarget::Tab(tab) = target {
            self.active_tab = Some(tab);
        }
        NavigationResult::Activated(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_default_to_off_and_can_be_changed_independently() {
        let mut preferences = AccessibilityPreferences::default();
        assert!(!preferences.high_contrast());
        assert!(!preferences.reduced_motion());

        preferences.set_high_contrast(true);
        assert!(preferences.high_contrast());
        assert!(!preferences.reduced_motion());
        preferences.toggle_reduced_motion();
        assert!(preferences.reduced_motion());
        preferences.toggle_high_contrast();
        assert!(!preferences.high_contrast());
    }

    #[test]
    fn focus_order_is_tabs_then_actions() {
        let navigation = KeyboardNavigation::new(3, 2);
        assert_eq!(
            navigation.focus_order(),
            &[
                FocusTarget::Tab(0),
                FocusTarget::Tab(1),
                FocusTarget::Tab(2),
                FocusTarget::Action(0),
                FocusTarget::Action(1),
            ]
        );
        assert_eq!(navigation.focused(), Some(FocusTarget::Tab(0)));
        assert_eq!(navigation.active_tab(), Some(0));
    }

    #[test]
    fn tab_navigation_wraps_in_both_directions() {
        let mut navigation = KeyboardNavigation::new(2, 1);
        assert_eq!(
            navigation.handle_key(NavigationKey::ShiftTab),
            NavigationResult::Focused(FocusTarget::Action(0))
        );
        assert_eq!(
            navigation.handle_key(NavigationKey::Tab),
            NavigationResult::Focused(FocusTarget::Tab(0))
        );
        assert_eq!(
            navigation.handle_key(NavigationKey::End),
            NavigationResult::Focused(FocusTarget::Action(0))
        );
        assert_eq!(
            navigation.handle_key(NavigationKey::Home),
            NavigationResult::Focused(FocusTarget::Tab(0))
        );
    }

    #[test]
    fn activation_selects_tabs_and_reports_actions() {
        let mut navigation = KeyboardNavigation::new(2, 1);
        navigation.focus(FocusTarget::Tab(1));
        assert_eq!(
            navigation.handle_key(NavigationKey::Enter),
            NavigationResult::Activated(FocusTarget::Tab(1))
        );
        assert_eq!(navigation.active_tab(), Some(1));

        navigation.focus(FocusTarget::Action(0));
        assert_eq!(
            navigation.handle_key(NavigationKey::Space),
            NavigationResult::Activated(FocusTarget::Action(0))
        );
        assert_eq!(navigation.active_tab(), Some(1));
    }

    #[test]
    fn empty_navigation_is_safe() {
        let mut navigation = KeyboardNavigation::new(0, 0);
        assert_eq!(navigation.focused(), None);
        assert_eq!(
            navigation.handle_key(NavigationKey::Tab),
            NavigationResult::Ignored
        );
        assert!(!navigation.focus(FocusTarget::Tab(0)));
    }
}
