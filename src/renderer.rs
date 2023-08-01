use egui_extras::RetainedImage;
use image::ImageOutputFormat;
use log::info;
use sensor_core::RenderData;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use eframe::egui::ColorImage;

pub fn render_image(
    ui_display_image_handle: &Arc<Mutex<Option<RetainedImage>>>,
    render_data: RenderData,
) {
    // Measure deserialization and rendering time
    let start = std::time::Instant::now();

    let image_data =
        sensor_core::render_lcd_image(render_data.lcd_config, render_data.sensor_values);

    let lcd_render_time = std::time::Instant::now();
    info!(
        "Rendering took {} ms",
        lcd_render_time.duration_since(start).as_millis()
    );

    // Create a Vec<u8> buffer to write the image to it
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    image_data
        .write_to(&mut cursor, ImageOutputFormat::Bmp)
        .unwrap();

    // Log time it took to write image to buffer
    let write_image_to_buffer_time = std::time::Instant::now();
    info!(
        "Writing image to buffer took {} ms",
        write_image_to_buffer_time
            .duration_since(lcd_render_time)
            .as_millis()
    );

    let image = RetainedImage::from_image_bytes("test", buf.as_slice()).unwrap();

    // Log time it took to create RetainedImage
    let create_retained_image_time = std::time::Instant::now();
    info!(
        "Creating RetainedImage took {} ms",
        create_retained_image_time
            .duration_since(write_image_to_buffer_time)
            .as_millis()
    );

    // Write image data to ui mutex
    let mut mutex = ui_display_image_handle.lock().unwrap();
    *mutex = Some(image);

    info!("Total time: {} ms", create_retained_image_time.duration_since(start).as_millis());
    info!("---");
}
