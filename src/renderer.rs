use std::sync::{Arc, Mutex};

use eframe::egui::ColorImage;
use egui_extras::RetainedImage;
use log::info;
use sensor_core::RenderData;

pub fn render_image(
    ui_display_image_handle: &Arc<Mutex<Option<RetainedImage>>>,
    render_data: RenderData,
) {
    let start = std::time::Instant::now();

    let image_data =
        sensor_core::render_lcd_image(render_data.lcd_config, render_data.sensor_values);

    let lcd_render_time = std::time::Instant::now();
    info!(
        "Rendering took {} ms",
        lcd_render_time.duration_since(start).as_millis()
    );

    let image = RetainedImage::from_color_image(
        "rendered_image",
        ColorImage::from_rgba_unmultiplied(
            [image_data.width() as usize, image_data.height() as usize],
            image_data.as_raw(),
        ),
    );

    // Log time it took to create RetainedImage
    let create_retained_image_time = std::time::Instant::now();
    info!(
        "Creating RetainedImage took {} ms",
        create_retained_image_time
            .duration_since(lcd_render_time)
            .as_millis()
    );

    // Write image data to ui mutex
    let mut mutex = ui_display_image_handle.lock().unwrap();
    *mutex = Some(image);

    info!(
        "Total time: {} ms",
        create_retained_image_time.duration_since(start).as_millis()
    );
    info!("---");
}
