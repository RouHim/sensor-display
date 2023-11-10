use std::{env, fs, thread};
use std::ops::Deref;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use eframe::egui::{ImageSource, Vec2};
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
    let image_data_mutex: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));

    // Create new thread to listen for tcp messages
    let write_image_data_mutex = image_data_mutex.clone();
    thread::spawn(move || {
        let (_handler, listener) = tcp_receiver::listen();
        tcp_receiver::receive(write_image_data_mutex, listener);
    });

    let ip = get_local_ip_address().join(", ").clone();
    let hostname = hostname::get().unwrap().into_string().unwrap();

    let show_start_screen = Arc::new(Mutex::new(true));
    let current_frame_number: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    eframe::run_simple_native("Sensor Display", options, move |ctx, _frame| {
        let resolution = format!(
            "{}x{}",
            ctx.screen_rect().width(),
            ctx.screen_rect().height()
        );

        egui_extras::install_image_loaders(ctx);
        ctx.request_repaint_after(Duration::from_millis(250));
        ctx.set_cursor_icon(egui::CursorIcon::None);

        egui::Area::new("main_area")
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let mut image_mutex = image_data_mutex.lock().unwrap();
                if let Some(image_data) = image_mutex.deref() {
                    // get current frame number
                    let mut current_number = current_frame_number.lock().unwrap();
                    let old_showing_frame = current_number.unwrap_or_default();

                    let frame_number = ctx.frame_nr();
                    let image_source =
                        ImageSource::from((format!("bytes://{frame_number}.jpg"), image_data.clone()));
                    let image = egui::Image::new(image_source).fit_to_exact_size(Vec2::new(
                        ctx.screen_rect().width(),
                        ctx.screen_rect().height(),
                    ));
                    ui.add(image);

                    // set image_mutex to none
                    *image_mutex = None;

                    // Set current frame number
                    *current_number = Some(frame_number);

                    // Remove the image cache for old curren showing frame number
                    ctx.forget_image(format!("bytes://{old_showing_frame}.jpg").as_str());

                    // set show_start_screen to false
                    let mut mutex = show_start_screen.lock().unwrap();
                    *mutex = false;

                } else if *show_start_screen.lock().unwrap() {
                    ui.label(&build_standby_text(&ip, &hostname, &resolution));
                }
                // Just show the cached image
                else {
                    let frame_number = current_frame_number.lock().unwrap().unwrap();
                    let image_source = ImageSource::from((format!("bytes://{frame_number}.jpg"), Vec::new()));
                    let image = egui::Image::new(image_source).fit_to_exact_size(Vec2::new(
                        ctx.screen_rect().width(),
                        ctx.screen_rect().height(),
                    ));
                    ui.add(image);
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
        .update_extended();

    if status.is_ok() && status.unwrap().updated() {
        info!("Respawning after update...");

        let current_exe = env::current_exe();
        let mut command = Command::new(current_exe.unwrap());
        command.args(env::args().skip(1));

        #[cfg(unix)]
        {
            let _err = command.exec();
        }

        #[cfg(windows)]
        {
            let _status = command.spawn().and_then(|mut c| c.wait()).unwrap();
        }
    }
}
