#[cfg(not(test))]
pub use master_time::{health, measurement, service};

#[cfg(not(test))]
mod app;

#[cfg(not(test))]
fn main() -> eframe::Result {
    app::run()
}

#[cfg(test)]
fn main() {}
