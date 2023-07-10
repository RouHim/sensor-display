mod tcp_receiver;

use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;

use egui_extras::RetainedImage;

fn main() -> Result<(), eframe::Error> {
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
                    ui.label("No image data available");
                }
            });
    })
}
