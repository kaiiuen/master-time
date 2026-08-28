//! The desktop shell for Master Time.

use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime};

use eframe::egui;
use master_time::ConfigServerProfile;
use master_time::calibration::Calibration;
use master_time::clock_display::ClockDisplayModel;
use master_time::config::{AppConfig, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL};
use master_time::diagnostics_view::DiagnosticsView;
use master_time::global_servers::GlobalServerCatalog;
use master_time::localization::{English, Key};
use master_time::polling::{PollEvent, PollingWorker};
use master_time::server_manager::ServerManager;
use master_time::servers::{Category, ServerCatalog, ServerProfile as PollingServerProfile};
use master_time::settings::{Language, LocalSettings, SettingsModel, Theme};
use master_time::state::{ApplicationState, DEFAULT_HISTORY_CAPACITY};
use master_time::sync_policy::{SyncDisposition, SyncPolicy};

#[path = "ui.rs"]
mod presentation;

const WINDOW_TITLE: &str = "Master Time";
const TABS: [&str; 7] = [
    "Time",
    "Server",
    "Network",
    "Calibration",
    "Diagnostics",
    "Global Servers",
    "Settings",
];

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
}

impl MasterTimeApp {
    fn new() -> Self {
        let catalog = ServerCatalog::built_in();
        let first_server = catalog
            .entries()
            .first()
            .expect("catalog is not empty")
            .profile();
        let profile = ConfigServerProfile::new(first_server.name(), first_server.hostname())
            .expect("built-in server profile must be valid");
        let mut config = AppConfig::default();
        config.add_server(profile);
        let state = ApplicationState::new(config, DEFAULT_HISTORY_CAPACITY);
        let manager = ServerManager::new(
            ApplicationState::new(state.config().clone(), DEFAULT_HISTORY_CAPACITY),
            catalog,
        );
        let local_settings = LocalSettings::default();
        let settings = SettingsModel::new(state.config(), local_settings);
        let interval_text = settings.draft().polling_interval().as_secs().to_string();

        Self {
            state,
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
            Err(error) => self.control_error = Some(error.to_string()),
        }
    }

    fn stop_polling(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.request_shutdown();
        }
        self.events = None;
    }

    fn receive_events(&mut self) {
        let Some(events) = self.events.take() else {
            return;
        };
        while let Ok(event) = events.try_recv() {
            match event {
                PollEvent::Success { result, .. } => self.state.apply_success(result),
                PollEvent::Error { error, .. } => self.state.record_error(error),
            }
        }
        self.events = Some(events);
    }

    fn show_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let polling = self.worker.is_some();
            if ui
                .add_enabled(!polling, egui::Button::new("Start polling"))
                .clicked()
            {
                self.start_polling();
            }
            if ui
                .add_enabled(polling, egui::Button::new("Stop polling"))
                .clicked()
            {
                self.stop_polling();
            }
            ui.label(if polling {
                "Polling active"
            } else {
                "Polling stopped"
            });
            if let Some(server) = self.state.active_server() {
                ui.separator();
                ui.label(format!("Active: {}", server.name()));
            }
        });
        if let Some(error) = &self.control_error {
            ui.colored_label(egui::Color32::from_rgb(190, 60, 60), error);
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

    fn show_time(&self, ui: &mut egui::Ui) {
        let view = self.presentation();
        ui.heading("Master Time");
        ui.label(ClockDisplayModel::new(Some(SystemTime::now())).format());
        ui.separator();
        ui.heading(English::text(Key::SyncStatus));
        ui.label(format!("{} — {}", view.status.label, view.status.detail));
        ui.separator();
        ui.heading(English::text(Key::Metrics));
        self.metric_grid(ui, &view);
        if let Some(summary) = view.history.summary {
            ui.separator();
            ui.label(format!("History: {summary}"));
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
        ui.heading("Configured servers");
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
                    self.control_error = Some(error.to_string());
                }
            }
        }
        ui.separator();
        ui.label(format!(
            "{} configured profile(s)",
            self.state.config().servers().len()
        ));
        ui.label("Server profiles are validated before they enter application state.");
    }

    fn show_network(&self, ui: &mut egui::Ui) {
        ui.heading("Network");
        ui.label("NTP measurement and polling status");
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
        ui.heading("Clock calibration");
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
        ui.heading("Diagnostics");
        let snapshot = self.diagnostics.collect();
        let stratum = self
            .state
            .latest_measurement()
            .map(|result| result.header.stratum);
        let view = DiagnosticsView::new(
            &snapshot,
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
    }

    fn show_global_servers(&mut self, ui: &mut egui::Ui) {
        ui.heading("Global server catalog");
        ui.horizontal(|ui| {
            ui.label("Search");
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
                if ui.button("Use server").clicked() {
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
                    self.control_error = None;
                }
                Err(error) => self.control_error = Some(error.to_string()),
            }
        }
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.horizontal(|ui| {
            ui.label("Polling interval (seconds)");
            ui.text_edit_singleline(&mut self.interval_text);
            if ui.button("Set draft").clicked() {
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
            "Allowed range: {}–{} seconds",
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
        if ui.checkbox(&mut always_on_top, "Always on top").changed() {
            self.settings.draft_mut().set_always_on_top(always_on_top);
        }
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
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
                        self.state = ApplicationState::new(config, capacity);
                        self.local_settings = local;
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
            if ui.button("Cancel").clicked() {
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
            "Local settings: {:?}, {:?}",
            self.local_settings.theme(),
            self.local_settings.language()
        ));
    }
}

impl eframe::App for MasterTimeApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_events();
        context.request_repaint_after(Duration::from_millis(250));
        egui::CentralPanel::default().show(context, |ui| {
            self.show_controls(ui);
            ui.separator();
            ui.horizontal(|ui| {
                for (index, title) in TABS.iter().enumerate() {
                    if ui.selectable_label(self.tab == index, *title).clicked() {
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

impl Default for MasterTimeApp {
    fn default() -> Self {
        Self::new()
    }
}
