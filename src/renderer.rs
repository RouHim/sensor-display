use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use eframe::egui::ColorImage;
use egui_extras::RetainedImage;
use log::info;
use sensor_core::{RenderData, SensorValue};

const MAX_SENSOR_VALUE_HISTORY: usize = 1000;

pub fn render_image(
    ui_display_image_handle: &Arc<Mutex<Option<RetainedImage>>>,
    sensor_value_history: &Arc<Mutex<Vec<Vec<SensorValue>>>>,
    render_data: RenderData,
    fonts_data: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
) {
    let start = std::time::Instant::now();

    // Insert last sensor values into sensor value history
    let last_sensor_values = render_data.sensor_values;
    let mut sensor_value_history = sensor_value_history.lock().unwrap();
    sensor_value_history.insert(0, last_sensor_values);

    // Limit sensor value history to MAX_SENSOR_VALUE_HISTORY
    while sensor_value_history.len() > MAX_SENSOR_VALUE_HISTORY {
        sensor_value_history.pop();
    }

    let history_read_time = std::time::Instant::now();
    info!(
        "Reading sensor values history took {:?}",
        history_read_time.duration_since(start)
    );

    let image_data = sensor_core::render_lcd_image(
        render_data.display_config,
        &sensor_value_history,
        fonts_data.lock().unwrap().deref(),
    );

    let lcd_render_time = std::time::Instant::now();
    info!(
        "Rendering took {:?}",
        lcd_render_time.duration_since(history_read_time)
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
        "Creating RetainedImage took {:?}",
        create_retained_image_time.duration_since(lcd_render_time)
    );

    // Write image data to ui mutex
    let mut mutex = ui_display_image_handle.lock().unwrap();
    *mutex = Some(image);

    info!(
        "Total time: {:?}",
        create_retained_image_time.duration_since(start)
    );
    info!("---");
}
