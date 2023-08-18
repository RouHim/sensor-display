use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fs, thread};

use eframe::egui;
use eframe::egui::Context;
use egui_extras::RetainedImage;
use log::info;
use self_update::cargo_crate_version;

use crate::tcp_receiver::get_local_ip_address;

mod renderer;
mod tcp_receiver;

fn main() -> Result<(), eframe::Error> {
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

    eframe::run_simple_native("Sensor Display", options, move |ctx, _frame| {
        ctx.request_repaint_after(Duration::from_millis(100));
        ctx.set_cursor_icon(egui::CursorIcon::None);
        egui::Area::new("main_area")
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                // Get image data from mutex
                let mutex = image_data_mutex.lock().unwrap();
                let image = mutex.deref();

                if let Some(image) = image {
                    image.show_max_size(ui, ui.available_size());
                } else {
                    ui.label(&get_standby_text(ctx));
                }
            });
    })
}

fn get_standby_text(ctx: &Context) -> String {
    let local_ip = get_local_ip_address().join(", ");
    let hostname = hostname::get().unwrap();
    let display_resolution = format!(
        "{}x{}",
        ctx.screen_rect().width(),
        ctx.screen_rect().height()
    );

    format!(
        "No data received yet.\n\nVersion:\t\t\t\t\t\t{}\nIP Addresse:\t\t\t\t{}\nHostname:\t\t\t\t\t{}\nDisplay resolution:\t{}",
        cargo_crate_version!(),
        local_ip,
        hostname.to_str().unwrap(),
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
        .update();
    info!("Update status: `{:?}`!", status);
}
