use std::fs;
use std::io::Cursor;
use std::sync::{Arc, RwLock};

use crate::ui::SharedImageHandle;
use ab_glyph::FontVec;
use log::{info, warn};
use lru::LruCache;
use sensor_core::{RenderData, SensorValueHistory};

const MAX_SENSOR_VALUE_HISTORY: usize = 1000;

/// Load fonts required for rendering using the font cache
fn load_fonts_for_rendering(
    elements: &[sensor_core::ElementConfig],
    font_cache: &Arc<RwLock<LruCache<String, FontVec>>>,
) -> std::collections::HashMap<String, Vec<u8>> {
    use sensor_core::ElementType;
    use std::collections::HashMap;

    let mut fonts_data = HashMap::new();

    // Debug: Show cache base directory
    info!(
        "Cache base directory: {:?}",
        sensor_core::get_cache_base_dir()
    );

    // Find all unique font families used in text elements
    let mut required_fonts = std::collections::HashSet::new();
    for element in elements {
        if element.element_type == ElementType::Text {
            if let Some(text_config) = &element.text_config {
                required_fonts.insert(text_config.font_family.clone());
            }
        }
    }
    info!("Required fonts: {:?}", required_fonts);

    // Load each required font using the cache
    for font_family in required_fonts {
        let font_path = sensor_core::get_cache_dir(&font_family, &ElementType::Text);
        info!("Looking for font {} at path: {:?}", font_family, font_path);

        // Check if font is already parsed in cache
        let font_in_cache = {
            let mut cache = font_cache.write().unwrap();
            cache.get(&font_family).is_some()
        };

        // Always load font bytes from disk (required by sensor_core)
        match fs::read(&font_path) {
            Ok(font_bytes) => {
                fonts_data.insert(font_family.clone(), font_bytes.clone());

                // Only parse and cache if not already in cache
                if !font_in_cache {
                    match FontVec::try_from_vec(font_bytes) {
                        Ok(parsed_font) => {
                            // Store in cache for future use
                            let mut cache = font_cache.write().unwrap();
                            cache.put(font_family.clone(), parsed_font);
                        }
                        Err(err) => {
                            warn!("Failed to parse font {}: {}", font_family, err);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to load font {}: {}", font_family, e);
            }
        }
    }

    fonts_data
}

pub fn render_image(
    ui_display_image_handle: &SharedImageHandle,
    sensor_value_history: &Arc<RwLock<SensorValueHistory>>,
    font_cache: &Arc<RwLock<LruCache<String, FontVec>>>,
    render_data: RenderData,
    image_width: u16,
    image_height: u16,
) {
    let start = std::time::Instant::now();

    // Insert last sensor values into sensor value history (newest first)
    let last_sensor_values = render_data.sensor_values;
    let mut sensor_value_history = sensor_value_history.write().unwrap();
    sensor_value_history.push_front(last_sensor_values);

    // Limit sensor value history to MAX_SENSOR_VALUE_HISTORY
    while sensor_value_history.len() > MAX_SENSOR_VALUE_HISTORY {
        sensor_value_history.pop_back();
    }

    let history_read_time = std::time::Instant::now();
    info!(
        "Reading sensor values history took {:?}",
        history_read_time.duration_since(start)
    );

    // Load fonts on-demand from filesystem
    let fonts_data = load_fonts_for_rendering(&render_data.elements, font_cache);

    let image_buffer = sensor_core::render_lcd_image(
        &render_data.elements,
        &sensor_value_history,
        &fonts_data,
        image_width,
        image_height,
    );

    let lcd_render_time = std::time::Instant::now();
    info!(
        "Rendering took {:?}",
        lcd_render_time.duration_since(history_read_time)
    );

    // Render to jpg
    let image_data = encode_jpeg(&image_buffer);

    // Current unix timestamp
    let unix_timestamp_nano = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // Write image data to ui mutex
    let mut mutex = ui_display_image_handle.write().unwrap();
    *mutex = Some((unix_timestamp_nano, image_data));

    info!("Total time: {:?}", lcd_render_time.duration_since(start));
    info!("---");
}

/// Encodes a rendered frame as JPEG with quality 100.
/// `image` 0.25 cannot encode RGBA buffers through `write_to(.., ImageFormat::Jpeg)`
/// (it returns `Unsupported` for RGBA) and the format-based path would silently
/// drop the quality to 75 - `JpegEncoder::encode_image` keeps both behavior and
/// quality, dropping the alpha channel like 0.24 did.
fn encode_jpeg(image: &image::RgbaImage) -> Vec<u8> {
    let mut image_data = Vec::new();
    let mut cursor = Cursor::new(&mut image_data);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 100)
        .encode_image(image)
        .expect("failed to encode the rendered frame as jpeg");
    image_data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_an_rgba_frame_as_decodable_jpeg() {
        let image = image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 0, 0, 255]));

        let bytes = encode_jpeg(&image);

        let decoded = image::load_from_memory(&bytes).expect("frame must be a decodable JPEG");
        assert_eq!((decoded.width(), decoded.height()), (8, 4));
    }
}
