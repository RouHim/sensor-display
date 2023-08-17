use std::ops::Deref;
use std::{fs, thread};

use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use egui_extras::RetainedImage;

use crate::tcp_receiver::get_local_ip_address;

mod renderer;
mod tcp_receiver;

fn main() -> Result<(), eframe::Error> {
    // Initialize the logger
    env_logger::init();

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

    let local_ip = get_local_ip_address().join(", ");
    let hostname = hostname::get().unwrap();
    let standby_text = format!(
        "No data received yet.\n\nIP Addresses:\t{}\nHostname:\t\t{}",
        local_ip,
        hostname.to_str().unwrap()
    );

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
                    ui.label(&standby_text);
                }
            });
    })
}
