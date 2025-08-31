use std::num::NonZeroUsize;
use std::ops::Deref;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use eframe::egui;
use eframe::egui::{ImageSource, Vec2};
use lru::LruCache;

use crate::http_client::get_local_ip_address;

pub type ImageData = Vec<u8>;
pub type ImageHandle = Option<(u128, ImageData)>;
pub type SharedImageHandle = Arc<RwLock<ImageHandle>>;

/// LRU cache for renderer assets to reduce disk I/O
const FONT_CACHE_SIZE: usize = 3;
type FontCache = LruCache<String, rusttype::Font<'static>>;

/// Builds the standby text
fn build_standby_text(local_ip: &str, hostname: &str, display_resolution: &str) -> String {
    format!(
        "No data received yet.\n\nVersion:\t\t\t\t\t\t{}\nIP Addresse:\t\t\t\t{}\nHostname:\t\t\t\t\t{}\nDisplay resolution:\t{}",
        self_update::cargo_crate_version!(),
        local_ip,
        hostname,
        display_resolution
    )
}

/// Main UI rendering function
pub fn run_ui(server_host: String, server_port: Option<u16>) -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_active(true)
            .with_decorations(false)
            .with_fullscreen(true)
            .with_drag_and_drop(false),
        ..Default::default()
    };

    // Create handler for asynchronous image data rendering
    let image_data_mutex: SharedImageHandle = Arc::new(RwLock::new(None));

    // Create LRU cache for renderer assets
    let font_cache: Arc<RwLock<FontCache>> = Arc::new(RwLock::new(LruCache::new(
        NonZeroUsize::new(FONT_CACHE_SIZE).unwrap(),
    )));

    // Get display resolution from egui context - we'll start the HTTP client after the first frame
    let write_image_data_mutex = image_data_mutex.clone();

    let mut ip = get_local_ip_address().join(", ").trim().to_string().clone();
    let hostname = hostname::get().unwrap().into_string().unwrap();

    // Holds the ids (timestamps) of the cached images
    let cached_image_index: Arc<RwLock<Vec<u128>>> = Arc::new(RwLock::new(Vec::new()));

    // Track if HTTP client has been started
    let http_client_started = Arc::new(RwLock::new(false));

    // Render loop
    eframe::run_simple_native("Sensor Display", native_options, move |ctx, _frame| {
        let display_width = ctx.screen_rect().width() as u16;
        let display_height = ctx.screen_rect().height() as u16;
        let resolution = format!("{}x{}", display_width, display_height);

        // Start HTTP client on first frame when we have the screen resolution
        let mut client_started = http_client_started.write().unwrap();
        if !*client_started {
            log::info!("Starting HTTP client");

            crate::http_client::start_http_client(
                write_image_data_mutex.clone(),
                font_cache.clone(),
                server_host.clone(),
                server_port,
                (display_width, display_height),
            );

            *client_started = true;
            log::info!(
                "HTTP client started. Server: {}:{}",
                server_host,
                server_port.unwrap_or(8080)
            );
        }

        // Install image loaders
        egui_extras::install_image_loaders(ctx);

        // Do not show the cursor
        ctx.set_cursor_icon(egui::CursorIcon::None);

        // Reduced display update frequency to reduce system load
        ctx.request_repaint_after(Duration::from_millis(250));

        egui::Area::new("main_area")
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let mut image_mutex = image_data_mutex.write().unwrap();
                let mut cached_image_index = cached_image_index.write().unwrap();

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
                    ui.label(build_standby_text(&ip, &hostname, &resolution));
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
