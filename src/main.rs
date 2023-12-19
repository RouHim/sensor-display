use std::ops::Deref;
use std::{env, fs, thread};

use std::sync::{Arc, LockResult, Mutex};
use std::time::Duration;

use crate::ignore_poison_lock::LockResultExt;
use eframe::egui;
use eframe::egui::{ImageSource, Vec2};
use self_update::cargo_crate_version;

use crate::tcp_receiver::get_local_ip_address;

mod ignore_poison_lock;
mod renderer;
mod tcp_receiver;
mod updater;

type ImageData = Vec<u8>;
type ImageHandle = Option<(u128, ImageData)>;
type SharedImageHandle = Arc<Mutex<ImageHandle>>;

fn main() -> Result<(), eframe::Error> {
    // Set the app name for the dynamic cache folder detection
    std::env::set_var("SENSOR_BRIDGE_APP_NAME", "sensor-display");

    // Initialize the logger
    env_logger::init();

    // Check for updates
    updater::update();

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

    // Create handler for asynchronous image data rendering
    let image_data_mutex: SharedImageHandle = Arc::new(Mutex::new(None));

    // Create new thread to listen for tcp messages
    let write_image_data_mutex = image_data_mutex.clone();
    thread::spawn(move || {
        let (_handler, listener) = tcp_receiver::listen();
        tcp_receiver::receive(write_image_data_mutex, listener);
    });

    let mut ip = get_local_ip_address().join(", ").trim().to_string().clone();
    let hostname = hostname::get().unwrap().into_string().unwrap();

    // Holds the ids (timestamps) of the cached images
    let cached_image_index: Arc<Mutex<Vec<u128>>> = Arc::new(Mutex::new(Vec::new()));

    // Render loop
    eframe::run_simple_native("Sensor Display", options, move |ctx, _frame| {
        let resolution = format!(
            "{}x{}",
            ctx.screen_rect().width() as i16,
            ctx.screen_rect().height() as i16
        );

        // Install image loaders
        egui_extras::install_image_loaders(ctx);

        // Do not show the cursor
        ctx.set_cursor_icon(egui::CursorIcon::None);

        // Reduced display update frequency to reduce system load
        ctx.request_repaint_after(Duration::from_millis(250));

        egui::Area::new("main_area")
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let mut image_mutex = image_data_mutex.lock().ignore_poison();
                let mut cached_image_index = cached_image_index.lock().ignore_poison();

                // A new image was rendered
                if let Some(image_data) = image_mutex.deref() {
                    // get timestamp of the new rendered image
                    let render_timestamp = image_data.0;

                    // Show new rendered image on the screen (also caches the image by name in memory)
                    let image_source = ImageSource::from((
                        format!("bytes://{render_timestamp}.jpg"),
                        image_data.1.clone(),
                    ));
                    let image = egui::Image::new(image_source).fit_to_exact_size(Vec2::new(
                        ctx.screen_rect().width(),
                        ctx.screen_rect().height(),
                    ));
                    ui.add(image);

                    // Set image mutex to none / consumed
                    *image_mutex = None;

                    // Insert the current showing image data id to the beginning of cached image ids
                    cached_image_index.insert(0, render_timestamp);

                    // Remove all images, expect the first one from the cache
                    cached_image_index.iter().skip(1).for_each(|cache_entry| {
                        ctx.forget_image(format!("bytes://{cache_entry}.jpg").as_str());
                    });

                    // Remove all ids expect the first one from the cache index
                    cached_image_index.truncate(1);
                }
                // No new freshly rendered image or cached image available, show standby text
                else if cached_image_index.is_empty() {
                    if ip.is_empty() {
                        ip = get_local_ip_address().join(", ").trim().to_string().clone();
                    }
                    ui.label(&build_standby_text(&ip, &hostname, &resolution));
                }
                // Show the cached image
                else {
                    let frame_number = cached_image_index.first().unwrap();
                    let image_source =
                        ImageSource::from((format!("bytes://{frame_number}.jpg"), Vec::new()));
                    let image = egui::Image::new(image_source).fit_to_exact_size(Vec2::new(
                        ctx.screen_rect().width(),
                        ctx.screen_rect().height(),
                    ));
                    ui.add(image);
                }
            });
    })
}

/// Builds the standby text
fn build_standby_text(local_ip: &str, hostname: &str, display_resolution: &str) -> String {
    format!(
        "No data received yet.\n\nVersion:\t\t\t\t\t\t{}\nIP Addresse:\t\t\t\t{}\nHostname:\t\t\t\t\t{}\nDisplay resolution:\t{}",
        cargo_crate_version!(),
        local_ip,
        hostname,
        display_resolution
    )
}
