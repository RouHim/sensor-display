use std::collections::HashMap;
use std::io::Cursor;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use crate::SharedImageHandle;
use log::info;
use sensor_core::{RenderData, SensorValue};

const MAX_SENSOR_VALUE_HISTORY: usize = 1000;

pub fn render_image(
    ui_display_image_handle: &SharedImageHandle,
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

    let image_buffer = sensor_core::render_lcd_image(
        render_data.display_config,
        &sensor_value_history,
        fonts_data.lock().unwrap().deref(),
    );

    let lcd_render_time = std::time::Instant::now();
    info!(
        "Rendering took {:?}",
        lcd_render_time.duration_since(history_read_time)
    );

    // Render to jpg
    let mut image_data = Vec::new();
    let mut cursor = Cursor::new(&mut image_data);
    image_buffer
        .write_to(&mut cursor, image::ImageOutputFormat::Jpeg(100))
        .unwrap();

    // Current unix timestamp
    let unix_timestamp_nano = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // Write image data to ui mutex
    let mut mutex = ui_display_image_handle.lock().unwrap();
    *mutex = Some((unix_timestamp_nano, image_data));

    info!("Total time: {:?}", lcd_render_time.duration_since(start));
    info!("---");
}
