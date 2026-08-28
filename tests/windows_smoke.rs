//! Smoke tests for the Windows platform boundaries.
//!
//! These tests intentionally exercise only dry-run data paths and menu-command
//! mapping. They do not apply a clock correction or initialize a tray backend.

use master_time::{
    ApprovedCorrection, PlatformTimeAdapter, PlatformTimeError, SystemTrayBackend,
    TrayBackendAvailability, TrayBackendLifecycle, TrayCommand, command_for_menu_id,
};

#[test]
fn platform_time_preview_is_a_dry_run() {
    let correction = ApprovedCorrection { offset: 0.25 };
    let preview = PlatformTimeAdapter::new()
        .preview(correction)
        .expect("a finite correction should be previewable");

    assert_eq!(preview.offset, correction.offset);
    assert!(preview.dry_run);
}

#[test]
fn platform_time_preview_rejects_non_finite_corrections() {
    let result = PlatformTimeAdapter::new().preview(ApprovedCorrection { offset: f64::NAN });

    assert_eq!(result, Err(PlatformTimeError::InvalidCorrection));
}

#[test]
fn tray_backend_starts_uninitialized_without_native_side_effects() {
    let backend = SystemTrayBackend::new(master_time::SystemTrayState::new());

    assert_eq!(backend.lifecycle(), TrayBackendLifecycle::Created);
    assert!(backend.model().window_visible());
    assert!(!backend.model().polling());
}

#[test]
fn tray_menu_ids_map_without_creating_a_tray_icon() {
    let expected = [
        (100, TrayCommand::Show),
        (101, TrayCommand::Hide),
        (102, TrayCommand::StartPolling),
        (103, TrayCommand::StopPolling),
        (104, TrayCommand::Quit),
    ];

    for (menu_id, command) in expected {
        assert_eq!(command_for_menu_id(menu_id), Some(command));
    }
    assert_eq!(command_for_menu_id(0), None);
    assert_eq!(command_for_menu_id(u16::MAX), None);
}

#[cfg(windows)]
#[test]
fn windows_reports_native_tray_boundary_without_initializing_it() {
    assert_eq!(
        SystemTrayBackend::availability(),
        TrayBackendAvailability::WindowsBoundary
    );
}

#[cfg(not(windows))]
#[test]
fn non_windows_reports_unsupported_tray_backend() {
    assert_eq!(
        SystemTrayBackend::availability(),
        TrayBackendAvailability::Unsupported
    );
}
