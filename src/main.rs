mod tcp_receiver;




use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::thread;


// create image reader


use eframe::egui;

use egui_extras::RetainedImage;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
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

    eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
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
