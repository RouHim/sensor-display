use std::{env, fs};

mod http_client;
mod renderer;
mod static_data;
mod ui;
mod updater;

fn main() -> Result<(), eframe::Error> {
    // Set the app name for the dynamic cache folder detection
    env::set_var("SENSOR_BRIDGE_APP_NAME", "sensor-display");

    // Initialize the logger
    env_logger::init();

    // Check for updates
    updater::update();

    // Cleanup data directory
    fs::remove_dir_all(sensor_core::get_cache_base_dir()).unwrap_or_default(); // Ignore errors

    // Get server configuration from environment variables or use defaults
    let server_host = env::var("SENSOR_BRIDGE_HOST").unwrap_or_else(|_| "localhost".to_string());
    let server_port = env::var("SENSOR_BRIDGE_PORT")
        .ok()
        .and_then(|port| port.parse().ok());

    // Run the UI
    ui::run_ui(server_host, server_port)
}
