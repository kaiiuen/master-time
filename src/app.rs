//! The first desktop shell for Master Time.

use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime};

use eframe::egui;
use master_time::config::{AppConfig, ServerProfile as ConfigServerProfile};
use master_time::polling::{PollEvent, PollingWorker};
use master_time::servers::{ServerCatalog, ServerProfile as PollingServerProfile};
use master_time::state::{ApplicationState, DEFAULT_HISTORY_CAPACITY};

#[path = "ui.rs"]
mod presentation;

const WINDOW_TITLE: &str = "Master Time";

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 520.0])
            .with_min_inner_size([420.0, 360.0])
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
}

impl MasterTimeApp {
    fn new() -> Self {
        let catalog = ServerCatalog::built_in();
        let first_server = catalog
            .entries()
            .first()
            .expect("built-in server catalog must not be empty")
            .profile();
        let profile = ConfigServerProfile::new(first_server.name(), first_server.hostname())
            .expect("built-in server profile must be valid");

        let mut config = AppConfig::default();
        config.add_server(profile);

        Self {
            state: ApplicationState::new(config, DEFAULT_HISTORY_CAPACITY),
            worker: None,
            events: None,
            control_error: None,
        }
    }

    fn start_polling(&mut self) {
        let Some(server) = self.state.active_server() else {
            self.control_error = Some("No server is configured".to_owned());
            return;
        };
        let profile = PollingServerProfile::from_strings(server.name(), server.hostname(), None)
            .expect("state contains a validated server profile");
        let interval = self.state.config().polling().interval();

        match PollingWorker::start(profile, interval) {
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
            let is_polling = self.worker.is_some();
            if ui
                .add_enabled(!is_polling, egui::Button::new("Start polling"))
                .clicked()
            {
                self.start_polling();
            }
            if ui
                .add_enabled(is_polling, egui::Button::new("Stop polling"))
                .clicked()
            {
                self.stop_polling();
            }
            ui.label(if is_polling {
                "Polling active"
            } else {
                "Polling stopped"
            });
        });
        if let Some(error) = &self.control_error {
            ui.colored_label(egui::Color32::from_rgb(190, 60, 60), error);
        }
    }

    fn show_presentation(&self, ui: &mut egui::Ui) {
        let view = presentation::present(presentation::ApplicationState {
            current_time: Some(SystemTime::now()),
            measurement: self.state.latest_measurement(),
            history: Some(self.state.history()),
            error: self.state.connection_error(),
        });

        ui.heading("Master Time");
        ui.label(format!("Local time: {}", view.current_time.value));
        ui.separator();
        ui.heading("Synchronization");
        ui.label(format!("Status: {}", view.status.label));
        ui.label(view.status.detail);
        ui.separator();
        ui.heading("Server and metrics");
        if let Some(server) = self.state.active_server() {
            ui.label(format!(
                "Current server: {} ({})",
                server.name(),
                server.hostname()
            ));
        }
        egui::Grid::new("server-metrics").show(ui, |ui| {
            ui.label("Server");
            ui.label(view.server_metrics.server);
            ui.end_row();
            ui.label("Stratum");
            ui.label(view.server_metrics.stratum);
            ui.end_row();
            ui.label("Offset");
            ui.label(view.server_metrics.offset);
            ui.end_row();
            ui.label("Round-trip delay");
            ui.label(view.server_metrics.round_trip_delay);
            ui.end_row();
            ui.label("Root distance");
            ui.label(view.server_metrics.root_distance);
            ui.end_row();
        });
        if let Some(error) = view.errors.message {
            ui.separator();
            ui.colored_label(egui::Color32::from_rgb(190, 60, 60), error);
        }
    }
}

impl eframe::App for MasterTimeApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_events();
        context.request_repaint_after(Duration::from_millis(250));

        egui::CentralPanel::default().show(context, |ui| {
            self.show_controls(ui);
            ui.add_space(8.0);
            self.show_presentation(ui);
        });
    }
}

impl Default for MasterTimeApp {
    fn default() -> Self {
        Self::new()
    }
}
