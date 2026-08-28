//! The desktop shell for Master Time.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime};

use eframe::egui;
use master_time::calibration::Calibration;
use master_time::chart::ChartRenderer;
use master_time::clock_display::ClockDisplayModel;
use master_time::config::{
    AppConfig, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL, ServerProfile as ConfigServerProfile,
};
use master_time::diagnostic_export::{DiagnosticSnapshot, ServerInfo};
use master_time::diagnostics_view::DiagnosticsView;
use master_time::global_servers::GlobalServerCatalog;
use master_time::history_view::ChartModel;

use master_time::persistence::PersistenceManager;
use master_time::polling::{PollEvent, PollingWorker};

use master_time::accessibility::{
    FocusTarget, KeyboardNavigation, NavigationKey, NavigationResult,
};
use master_time::measurement::MeasurementHistory;
use master_time::server_manager::ServerManager;
use master_time::servers::{Category, ServerCatalog, ServerProfile as PollingServerProfile};
use master_time::settings::{Language, LocalSettings, SettingsModel, Theme};
use master_time::state::{ApplicationState, DEFAULT_HISTORY_CAPACITY};
use master_time::sync_policy::{SyncDisposition, SyncPolicy};
use master_time::system_tray::{SystemTrayState, TrayAction};
use master_time::time_action::{CorrectionRequest, TimeAction};
use master_time::translations::{Key, Language as TranslationLanguage, TranslationCatalog};
use master_time::{PlatformTimeAdapter, SystemTrayBackend, TrayBackendAvailability};

#[path = "ui.rs"]
mod presentation;

const WINDOW_TITLE: &str = "Master Time";

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 600.0])
            .with_min_inner_size([520.0, 420.0])
            .with_title(WINDOW_TITLE),
        ..Default::default()
    };

    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|_creation_context| Ok(Box::new(MasterTimeApp::new()))),
    )
}

struct MasterTimeApp {
    state: ApplicationState,
    persistence: PersistenceManager,
    worker: Option<PollingWorker>,
    events: Option<Receiver<PollEvent>>,
    control_error: Option<String>,
    tab: usize,
    server_manager: ServerManager,
    global_catalog: GlobalServerCatalog,
    catalog_query: String,
    catalog_category: Option<Category>,
    calibration: Calibration,
    diagnostics: master_time::DiagnosticsCollector,
    settings: SettingsModel,
    local_settings: LocalSettings,
    interval_text: String,
    delay_history: MeasurementHistory,

    recovery_notice: Option<String>,
    notification: Option<String>,
    diagnostic_export_location: Option<PathBuf>,
    time_action: TimeAction,
    platform_time: PlatformTimeAdapter,
    correction_opt_in: bool,
    tray: SystemTrayBackend,
    navigation: KeyboardNavigation,
}

impl MasterTimeApp {
    fn new() -> Self {
        let catalog = ServerCatalog::built_in();
        let persistence_path = Self::persistence_path();
        let (mut persistence, mut load_error) =
            match PersistenceManager::load_or_default(&persistence_path) {
                Ok(manager) => (manager, None),
                Err(error) => (
                    PersistenceManager::new(&persistence_path, AppConfig::default()),
                    Some(error.to_string()),
                ),
            };
        let first_server = catalog
            .entries()
            .first()
            .expect("catalog is not empty")
            .profile();
        let profile = ConfigServerProfile::new(first_server.name(), first_server.hostname())
            .expect("built-in server profile must be valid");
        let mut config = persistence.config().clone();
        if config.servers().is_empty() {
            config.add_server(profile);
            if let Err(error) = persistence.set_config(config.clone()) {
                load_error = Some(error.to_string());
            }
        }
        let state = ApplicationState::new(config, DEFAULT_HISTORY_CAPACITY);
        let manager = ServerManager::new(
            ApplicationState::new(state.config().clone(), DEFAULT_HISTORY_CAPACITY),
            catalog,
        );
        let local_settings = LocalSettings::default();
        let settings = SettingsModel::new(state.config(), local_settings);
        let interval_text = settings.draft().polling_interval().as_secs().to_string();
        let mut tray = SystemTrayBackend::new(SystemTrayState::new());
        let tray_error = tray
            .initialize()
            .err()
            .map(|error| format!("System tray unavailable: {error:?}"));
        if load_error.is_none() {
            load_error = tray_error;
        }

        Self {
            state,
            persistence,
            worker: None,
            events: None,
            control_error: None,
            tab: 0,
            server_manager: manager,
            global_catalog: GlobalServerCatalog::built_in(),
            catalog_query: String::new(),
            catalog_category: None,
            calibration: Calibration::new(),
            diagnostics: master_time::DiagnosticsCollector::new(),
            settings,
            local_settings,
            interval_text,
            delay_history: MeasurementHistory::new(DEFAULT_HISTORY_CAPACITY),

            recovery_notice: None,
            notification: load_error,
            diagnostic_export_location: None,
            time_action: TimeAction::default(),
            platform_time: PlatformTimeAdapter::new(),
            correction_opt_in: false,
            tray,
            navigation: KeyboardNavigation::new(7, 0),
        }
    }

    fn persistence_path() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("master-time.conf")
    }

    fn tr(&self, key: Key) -> &'static str {
        let language = match self.settings.draft().language() {
            Language::English => TranslationLanguage::English,
            Language::SimplifiedChinese => TranslationLanguage::SimplifiedChinese,
            Language::TraditionalChinese => TranslationLanguage::TraditionalChinese,
        };
        TranslationCatalog::new(language).text(key)
    }

    fn notify(&mut self, message: impl Into<String>) {
        self.notification = Some(message.into());
    }

    fn sync_persistence(&mut self) {
        let config = self.state.config().clone();
        if let Err(error) = self.persistence.set_config(config) {
            self.notify(error.to_string());
        }
        if let Err(error) = self.persistence.save_if_dirty() {
            self.notify(error.to_string());
        }
    }

    fn start_polling(&mut self) {
        let Some(server) = self.state.active_server() else {
            self.control_error = Some("No server is configured".to_owned());
            return;
        };
        let profile = PollingServerProfile::from_strings(server.name(), server.hostname(), None)
            .expect("state contains a validated server profile");
        match PollingWorker::start(profile, self.state.config().polling().interval()) {
            Ok((worker, events)) => {
                self.state.begin_polling();
                self.worker = Some(worker);
                self.events = Some(events);
                self.control_error = None;
            }
            Err(error) => self.notify(error.to_string()),
        }
    }

    fn stop_polling(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.request_shutdown();
        }
        self.events = None;
        if let Err(error) = self.persistence.save_if_dirty() {
            self.notify(error.to_string());
        }
    }

    fn receive_tray_commands(&mut self, context: &egui::Context) {
        if SystemTrayBackend::availability() == TrayBackendAvailability::Unsupported {
            return;
        }
        match self.tray.poll_native_commands() {
            Ok(Some(action)) => match action {
                TrayAction::Show => context.send_viewport_cmd(egui::ViewportCommand::Visible(true)),
                TrayAction::Hide => {
                    context.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                TrayAction::StartPolling => self.start_polling(),
                TrayAction::StopPolling => self.stop_polling(),
                TrayAction::Quit => {
                    self.stop_polling();
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                TrayAction::Noop => {}
            },
            Ok(None) => {}
            Err(error) => self.notify(format!("System tray error: {error:?}")),
        }
    }

    fn handle_keyboard(&mut self, context: &egui::Context) {
        let events = if context.wants_keyboard_input() {
            Vec::new()
        } else {
            context.input(|input| input.events.clone())
        };
        for event in events {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            let navigation_key = match key {
                egui::Key::Tab if modifiers.shift => Some(NavigationKey::ShiftTab),
                egui::Key::Tab => Some(NavigationKey::Tab),
                egui::Key::ArrowLeft => Some(NavigationKey::Left),
                egui::Key::ArrowRight => Some(NavigationKey::Right),
                egui::Key::ArrowUp => Some(NavigationKey::Up),
                egui::Key::ArrowDown => Some(NavigationKey::Down),
                egui::Key::Home => Some(NavigationKey::Home),
                egui::Key::End => Some(NavigationKey::End),
                egui::Key::Enter => Some(NavigationKey::Enter),
                egui::Key::Space => Some(NavigationKey::Space),
                _ => None,
            };
            if let Some(navigation_key) = navigation_key {
                if let NavigationResult::Activated(FocusTarget::Tab(tab)) =
                    self.navigation.handle_key(navigation_key)
                {
                    self.tab = tab;
                }
            }
        }
        if self.navigation.active_tab() != Some(self.tab) {
            let _ = self.navigation.focus(FocusTarget::Tab(self.tab));
        }
    }

    fn receive_events(&mut self) {
        let Some(events) = self.events.take() else {
            return;
        };
        while let Ok(event) = events.try_recv() {
            match event {
                PollEvent::Success {
                    result,
                    consecutive_failures,
                    ..
                } => {
                    self.delay_history.push(result.measurement.round_trip_delay);
                    self.state.apply_success(result);
                    self.recovery_notice = (consecutive_failures > 0).then(|| {
                        format!("Polling recovered after {consecutive_failures} failures")
                    });
                }
                PollEvent::Error {
                    error,
                    consecutive_failures,
                    retry_delay,
                    ..
                } => {
                    self.state.record_error(error);
                    self.recovery_notice = Some(format!(
                        "Polling failed ({consecutive_failures} consecutive); retrying in {}s",
                        retry_delay.map_or(0, |delay| delay.as_secs())
                    ));
                }
            }
        }
        self.events = Some(events);
    }

    fn show_controls(&mut self, ui: &mut egui::Ui) {
        let start_label = self.tr(Key::StartPolling);
        let stop_label = self.tr(Key::StopPolling);
        let active_label = self.tr(Key::PollingActive);
        let stopped_label = self.tr(Key::PollingStopped);
        ui.horizontal(|ui| {
            let polling = self.worker.is_some();
            if ui
                .add_enabled(!polling, egui::Button::new(start_label))
                .clicked()
            {
                self.start_polling();
            }
            if ui
                .add_enabled(polling, egui::Button::new(stop_label))
                .clicked()
            {
                self.stop_polling();
            }
            ui.label(if polling { active_label } else { stopped_label });
            if let Some(server) = self.state.active_server() {
                ui.separator();
                ui.label(format!("Active: {}", server.name()));
            }
        });
        if let Some(error) = &self.control_error {
            ui.colored_label(egui::Color32::from_rgb(190, 60, 60), error);
        }
        if let Some(notice) = &self.recovery_notice {
            ui.colored_label(egui::Color32::from_rgb(190, 150, 50), notice);
        }
        if let Some(notification) = &self.notification {
            ui.colored_label(egui::Color32::from_rgb(190, 150, 50), notification);
        }
    }

    fn presentation(&self) -> presentation::Presentation {
        presentation::present(presentation::ApplicationState {
            current_time: Some(SystemTime::now()),
            measurement: self.state.latest_measurement(),
            history: Some(self.state.history()),
            error: self.state.connection_error(),
        })
    }

    fn show_time(&mut self, ui: &mut egui::Ui) {
        let view = self.presentation();
        ui.heading(self.tr(Key::MasterTime));
        ui.label(ClockDisplayModel::new(Some(SystemTime::now())).format());
        ui.separator();
        ui.heading(self.tr(Key::SyncStatus));
        ui.label(format!("{} — {}", view.status.label, view.status.detail));
        ui.separator();
        ui.heading(self.tr(Key::Metrics));
        self.metric_grid(ui, &view);
        if let Some(summary) = view.history.summary {
            ui.separator();
            ui.label(format!("{}: {summary}", self.tr(Key::History)));
        }
        let chart = ChartModel::from_histories(self.state.history(), &self.delay_history);
        ChartRenderer::default().show(ui, &chart);
        let measurement = self
            .state
            .latest_measurement()
            .map(|result| result.measurement);
        let stratum = self
            .state
            .latest_measurement()
            .map_or(0, |result| result.header.stratum);
        let request = CorrectionRequest::new(measurement, self.state.health_status(), stratum);
        let preview = self.time_action.preview(request);
        ui.separator();
        ui.label(format!(
            "Safe correction preview: {}",
            match preview.offset {
                Some(offset) => format!("{offset:+.3}s ({:?})", preview.disposition),
                None => "no measurement".to_owned(),
            }
        ));
        if ui.button("Request safe correction preview").clicked() {
            match self.time_action.request_correction(request) {
                Ok(approved) => self.notify(format!(
                    "Correction approved for preview: {:+.3}s; no clock change was made",
                    approved.offset
                )),
                Err(error) => self.notify(error.to_string()),
            }
        }
        if ui
            .checkbox(
                &mut self.correction_opt_in,
                "Allow this session to change the Windows system clock",
            )
            .changed()
        {
            self.platform_time = if self.correction_opt_in {
                PlatformTimeAdapter::with_explicit_opt_in()
            } else {
                PlatformTimeAdapter::new()
            };
        }
        ui.label("Clock changes are never automatic and require the confirmation below.");
        if !cfg!(windows) {
            ui.label("System clock changes are unavailable on this platform.");
        }
        if ui
            .add_enabled(
                self.correction_opt_in && cfg!(windows),
                egui::Button::new("Confirm and apply approved correction"),
            )
            .clicked()
        {
            match self.time_action.request_correction(request) {
                Ok(approved) => match self.platform_time.apply(approved) {
                    Ok(applied) => self.notify(format!(
                        "System clock correction applied: {:+.3}s",
                        applied.offset
                    )),
                    Err(error) => self.notify(error.to_string()),
                },
                Err(error) => self.notify(error.to_string()),
            }
        }
        if let Some(error) = view.errors.message {
            ui.colored_label(egui::Color32::from_rgb(190, 60, 60), error);
        }
    }

    fn metric_grid(&self, ui: &mut egui::Ui, view: &presentation::Presentation) {
        egui::Grid::new("metrics").striped(true).show(ui, |ui| {
            for (label, value) in [
                ("Server", &view.server_metrics.server),
                ("Stratum", &view.server_metrics.stratum),
                ("Offset", &view.server_metrics.offset),
                ("Round-trip delay", &view.server_metrics.round_trip_delay),
                ("Root distance", &view.server_metrics.root_distance),
            ] {
                ui.label(label);
                ui.label(value);
                ui.end_row();
            }
        });
    }

    fn show_server(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.tr(Key::ConfiguredServers));
        let mut selected = self.state.config().active_server_index();
        for (index, server) in self.state.config().servers().iter().enumerate() {
            ui.radio_value(
                &mut selected,
                Some(index),
                format!("{} ({})", server.name(), server.hostname()),
            );
        }
        if selected != self.state.config().active_server_index() {
            if self.server_manager.select(selected).is_ok() {
                if let Err(error) = self.state.set_active_server(selected) {
                    self.notify(error.to_string());
                } else {
                    self.sync_persistence();
                }
            }
        }
        ui.separator();
        ui.label(format!(
            "{} {}",
            self.state.config().servers().len(),
            self.tr(Key::ConfiguredProfiles)
        ));
        ui.label(self.tr(Key::ServerProfilesValidated));
    }

    fn show_network(&self, ui: &mut egui::Ui) {
        ui.heading(self.tr(Key::Network));
        ui.label(self.tr(Key::NtpMeasurementPollingStatus));
        let view = self.presentation();
        self.metric_grid(ui, &view);
        ui.separator();
        let measurement = self
            .state
            .latest_measurement()
            .map(|result| result.measurement);
        let stratum = self
            .state
            .latest_measurement()
            .map_or(0, |result| result.header.stratum);
        let disposition =
            SyncPolicy::default().classify(measurement, self.state.health_status(), stratum);
        ui.label(format!(
            "Sync policy: {}",
            match disposition {
                SyncDisposition::EligibleForCorrection => "eligible for correction",
                SyncDisposition::DisplayOnly => "display only",
                SyncDisposition::Unsafe => "unsafe",
            }
        ));
    }

    fn show_calibration(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.tr(Key::ClockCalibration));
        let view = self.calibration.view();
        if !view.enabled {
            if ui.button("Begin calibration").clicked() {
                self.calibration.enable();
            }
            ui.label("Calibration waits for the next minute boundary.");
        } else {
            if let Some(countdown) = view.countdown {
                ui.label(format!(
                    "Time until boundary: {} seconds",
                    countdown.as_secs()
                ));
            }
            if ui.button("Mark boundary").clicked() {
                self.calibration.mark();
            }
            if ui.button("Stop calibration").clicked() {
                self.calibration.disable();
            }
        }
        if let Some(result) = view.result {
            ui.separator();
            ui.label(format!("Expected: {:?}", result.expected));
            ui.label(format!("Marked: {:?}", result.marked));
            ui.label(format!(
                "Difference: {:?} ({})",
                result.difference,
                if result.marked_after_expected {
                    "late"
                } else {
                    "early"
                }
            ));
        }
    }

    fn show_diagnostics(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.tr(Key::Diagnostics));
        let snapshot = self.diagnostic_snapshot();
        let stratum = self
            .state
            .latest_measurement()
            .map(|result| result.header.stratum);
        let view = DiagnosticsView::new(
            &snapshot.diagnostics,
            self.state
                .latest_measurement()
                .map(|result| result.measurement),
            stratum,
            self.state.health_status(),
        );
        egui::Grid::new("diagnostics").striped(true).show(ui, |ui| {
            for row in view.rows() {
                ui.label(row.label);
                ui.label(&row.value);
                ui.end_row();
            }
        });
        ui.separator();
        ui.label("Export diagnostics:");
        ui.horizontal(|ui| {
            if ui.button("Export plain text").clicked() {
                self.export_diagnostics(&snapshot, false);
            }
            if ui.button("Export JSON").clicked() {
                self.export_diagnostics(&snapshot, true);
            }
        });
        if let Some(path) = &self.diagnostic_export_location {
            ui.label(format!("Last export location: {}", path.display()));
        }
    }

    fn diagnostic_snapshot(&mut self) -> DiagnosticSnapshot {
        DiagnosticSnapshot {
            diagnostics: self.diagnostics.collect(),
            health: Some(self.state.health_status()),
            measurement: self
                .state
                .latest_measurement()
                .map(|result| result.measurement),
            server: self
                .state
                .active_server()
                .map(|server| ServerInfo::new(server.name(), server.hostname())),
            network: None,
        }
    }

    fn export_diagnostics(&mut self, snapshot: &DiagnosticSnapshot, json: bool) {
        let filename = if json {
            "master-time-diagnostics.json"
        } else {
            "master-time-diagnostics.txt"
        };
        let path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(filename);
        let format = if json { "JSON" } else { "plain-text" };
        let result = if json {
            snapshot.write_json(&path)
        } else {
            snapshot.write_plain_text(&path)
        };
        match result {
            Ok(()) => {
                self.diagnostic_export_location = Some(path.clone());
                self.notify(format!(
                    "Diagnostics exported as {format} to {}",
                    path.display()
                ));
            }
            Err(error) => self.notify(format!("Diagnostics export failed ({format}): {error}")),
        }
    }

    fn show_global_servers(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.tr(Key::GlobalServerCatalog));
        ui.horizontal(|ui| {
            ui.label(self.tr(Key::Search));
            ui.text_edit_singleline(&mut self.catalog_query);
            egui::ComboBox::from_id_salt("global-category")
                .selected_text(self.catalog_category.map_or("All", Category::as_str))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.catalog_category, None, "All");
                    for category in Category::ALL {
                        ui.selectable_value(
                            &mut self.catalog_category,
                            Some(category),
                            category.as_str(),
                        );
                    }
                });
        });
        let mut pending_server = None;
        for entry in self
            .global_catalog
            .filter(self.catalog_category, &self.catalog_query)
        {
            ui.group(|ui| {
                ui.label(format!(
                    "{} — {}",
                    entry.profile().name(),
                    entry.profile().hostname()
                ));
                ui.small(format!(
                    "{} · {} · {}",
                    entry.category().as_str(),
                    entry.strategy(),
                    entry.notes()
                ));
                if ui.button(self.tr(Key::UseServer)).clicked() {
                    pending_server = Some((
                        entry.profile().name().to_owned(),
                        entry.profile().hostname().to_owned(),
                    ));
                }
            });
        }
        if let Some((name, hostname)) = pending_server {
            self.stop_polling();
            match self.server_manager.add(name, hostname) {
                Ok(_) => {
                    let config = AppConfig::new(
                        self.server_manager.profiles().to_vec(),
                        self.server_manager.selected_index(),
                        self.state.config().polling(),
                    )
                    .expect("server manager maintains valid configuration");
                    self.state = ApplicationState::new(config, self.state.history().capacity());
                    self.sync_persistence();
                    self.control_error = None;
                }
                Err(error) => self.control_error = Some(error.to_string()),
            }
        }
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.tr(Key::Settings));
        ui.horizontal(|ui| {
            ui.label(self.tr(Key::PollingIntervalSeconds));
            ui.text_edit_singleline(&mut self.interval_text);
            if ui.button(self.tr(Key::SetDraft)).clicked() {
                match self.interval_text.trim().parse::<u64>() {
                    Ok(seconds) => {
                        if let Err(error) = self
                            .settings
                            .draft_mut()
                            .set_polling_interval(Duration::from_secs(seconds))
                        {
                            self.control_error = Some(error.to_string());
                        }
                    }
                    Err(_) => {
                        self.control_error = Some("Polling interval must be an integer".to_owned())
                    }
                }
            }
        });
        ui.label(format!(
            "{}: {}–{} seconds",
            self.tr(Key::AllowedRangeSeconds),
            MIN_POLL_INTERVAL.as_secs(),
            MAX_POLL_INTERVAL.as_secs()
        ));
        let mut theme = self.settings.draft().theme();
        egui::ComboBox::from_id_salt("theme")
            .selected_text(format!("Theme: {theme:?}"))
            .show_ui(ui, |ui| {
                for value in [Theme::System, Theme::Light, Theme::Dark] {
                    if ui
                        .selectable_value(&mut theme, value, format!("{value:?}"))
                        .clicked()
                    {
                        self.settings.draft_mut().set_theme(value);
                    }
                }
            });
        let mut language = self.settings.draft().language();
        egui::ComboBox::from_id_salt("language")
            .selected_text(format!("Language: {language:?}"))
            .show_ui(ui, |ui| {
                for value in [
                    Language::English,
                    Language::SimplifiedChinese,
                    Language::TraditionalChinese,
                ] {
                    if ui
                        .selectable_value(&mut language, value, format!("{value:?}"))
                        .clicked()
                    {
                        self.settings.draft_mut().set_language(value);
                    }
                }
            });
        let mut always_on_top = self.settings.draft().always_on_top();
        if ui
            .checkbox(&mut always_on_top, self.tr(Key::AlwaysOnTop))
            .changed()
        {
            self.settings.draft_mut().set_always_on_top(always_on_top);
        }
        ui.horizontal(|ui| {
            if ui.button(self.tr(Key::Apply)).clicked() {
                let interval_changed = self.settings.draft().polling_interval()
                    != self.state.config().polling().interval();
                let selection_changed = self.settings.draft().active_server()
                    != self.state.config().active_server_index();
                if interval_changed || selection_changed {
                    self.stop_polling();
                }
                let mut config = self.state.config().clone();
                let mut local = self.local_settings;
                match self.settings.apply(&mut config, &mut local) {
                    Ok(()) => {
                        let capacity = self.state.history().capacity();
                        let server_changed = self.state.config().active_server_index()
                            != config.active_server_index();
                        self.state = ApplicationState::new(config, capacity);
                        if server_changed {
                            self.delay_history = MeasurementHistory::new(capacity);
                        }
                        let _ = self
                            .server_manager
                            .select(self.state.config().active_server_index());
                        self.local_settings = local;
                        self.sync_persistence();
                        self.interval_text = self
                            .settings
                            .draft()
                            .polling_interval()
                            .as_secs()
                            .to_string();
                        self.control_error = None;
                    }
                    Err(error) => self.control_error = Some(error.to_string()),
                }
            }
            if ui.button(self.tr(Key::Cancel)).clicked() {
                self.settings.cancel();
                self.interval_text = self
                    .settings
                    .draft()
                    .polling_interval()
                    .as_secs()
                    .to_string();
                self.control_error = None;
            }
        });
        ui.label(format!(
            "{}: {:?}, {:?}",
            self.tr(Key::LocalSettings),
            self.local_settings.theme(),
            self.local_settings.language()
        ));
    }
}

impl eframe::App for MasterTimeApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_tray_commands(context);
        self.handle_keyboard(context);
        self.receive_events();
        context.request_repaint_after(Duration::from_millis(250));
        egui::CentralPanel::default().show(context, |ui| {
            self.show_controls(ui);
            ui.separator();
            ui.horizontal(|ui| {
                let tab_keys = [
                    Key::Time,
                    Key::Server,
                    Key::Network,
                    Key::Calibration,
                    Key::Diagnostics,
                    Key::GlobalServers,
                    Key::Settings,
                ];
                for (index, key) in tab_keys.into_iter().enumerate() {
                    if ui
                        .selectable_label(self.tab == index, self.tr(key))
                        .clicked()
                    {
                        self.tab = index;
                    }
                }
            });
            ui.separator();
            match self.tab {
                0 => self.show_time(ui),
                1 => self.show_server(ui),
                2 => self.show_network(ui),
                3 => self.show_calibration(ui),
                4 => self.show_diagnostics(ui),
                5 => self.show_global_servers(ui),
                6 => self.show_settings(ui),
                _ => unreachable!(),
            }
        });
    }
}

impl Drop for MasterTimeApp {
    fn drop(&mut self) {
        self.stop_polling();
        let _ = self.tray.shutdown();
    }
}

impl Default for MasterTimeApp {
    fn default() -> Self {
        Self::new()
    }
}
