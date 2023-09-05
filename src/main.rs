use std::ops::Deref;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{env, fs, thread};

use eframe::egui;
use egui_extras::RetainedImage;
use log::info;
use self_update::cargo_crate_version;

use crate::tcp_receiver::get_local_ip_address;

mod renderer;
mod tcp_receiver;

fn main() -> Result<(), eframe::Error> {
    // Set the app name for the dynamic cache folder detection
    std::env::set_var("SENSOR_BRIDGE_APP_NAME", "sensor-display");

    // Initialize the logger
    env_logger::init();

    // Check for updates
    update();

    // Cleanup data directory
    fs::remove_dir_all(sensor_core::get_cache_base_dir()).unwrap_or_default(); // Ignore errors

    // Fullscreen without border
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
        active: true,
        decorated: false,
        fullscreen: true,
        drag_and_drop_support: false,

        ..Default::default()
    };

    // Create arc mutex for image data
    let image_data_mutex: Arc<Mutex<Option<RetainedImage>>> = Arc::new(Mutex::new(None));

    // Create new thread to listen for tcp messages
    let write_image_data_mutex = image_data_mutex.clone();
    thread::spawn(move || {
        let (_handler, listener) = tcp_receiver::listen();
        tcp_receiver::receive(write_image_data_mutex, listener);
    });

    let ip = get_local_ip_address().join(", ").clone();
    let hostname = hostname::get().unwrap().into_string().unwrap();

    eframe::run_simple_native("Sensor Display", options, move |ctx, _frame| {
        let resolution = format!(
            "{}x{}",
            ctx.screen_rect().width(),
            ctx.screen_rect().height()
        );
        ctx.request_repaint_after(Duration::from_millis(25));
        ctx.set_cursor_icon(egui::CursorIcon::None);
        egui::Area::new("main_area")
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                // Get image data from mutex
                let mutex = image_data_mutex.lock().unwrap();
                if let Some(image) = mutex.deref() {
                    image.show_max_size(ui, ui.available_size());
                } else {
                    ui.label(&build_standby_text(&ip, &hostname, &resolution));
                }
            });
    })
}

fn build_standby_text(local_ip: &str, hostname: &str, display_resolution: &str) -> String {
    format!(
        "No data received yet.\n\nVersion:\t\t\t\t\t\t{}\nIP Addresse:\t\t\t\t{}\nHostname:\t\t\t\t\t{}\nDisplay resolution:\t{}",
        cargo_crate_version!(),
        local_ip,
        hostname,
        display_resolution
    )
}

/// Check for updates
/// If an update is available, download and install it
/// If no update is available, do nothing
/// Automatically restart the application after update
fn update() {
    // In release mode, don't ask for confirmation
    let no_confirm: bool = !cfg!(debug_assertions);

    let status = self_update::backends::github::Update::configure()
        .repo_owner("rouhim")
        .repo_name("sensor-display")
        .bin_name("sensor-display")
        .show_download_progress(true)
        .no_confirm(no_confirm)
        .current_version(cargo_crate_version!())
        .build()
        .unwrap()
        .update_extended()
        .unwrap();

    if status.updated() {
        info!("Respawning after update...");

        let current_exe = env::current_exe();
        let mut command = Command::new(current_exe.unwrap());
        command.args(env::args().skip(1));

        #[cfg(unix)]
        {
            let err = command.exec();
        }

        #[cfg(windows)]
        {
            let _status = command.spawn().and_then(|mut c| c.wait()).unwrap();
        }
    }
}
