use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use egui_extras::RetainedImage;
use log::{info, warn};
use message_io::network::{NetEvent, Transport};
use message_io::node::{self, NodeHandler, NodeListener};
use rayon::prelude::*;
use sensor_core::{
    ElementType, PrepareConditionalImageData, PrepareStaticImageData, RenderData, SensorValue,
    TransportMessage, TransportType,
};

use crate::renderer;

const PORT: u16 = 10489;

/// Opens a tcp socket to the specified address
/// Returns a handler and a listener
pub fn listen() -> (NodeHandler<()>, NodeListener<()>) {
    // Create a node that will listen for incoming network messages.
    let (handler, listener) = node::split::<()>();

    // Listen for TCP, UDP and WebSocket messages at the same time.
    handler
        .network()
        .listen(Transport::FramedTcp, format!("0.0.0.0:{PORT}"))
        .unwrap();

    (handler, listener)
}

pub fn receive(
    ui_display_image_handle: Arc<Mutex<Option<RetainedImage>>>,
    listener: NodeListener<()>,
) {
    let render_busy_indicator = Arc::new(Mutex::new(false));
    let sensor_value_history: Arc<Mutex<Vec<Vec<SensorValue>>>> = Arc::new(Mutex::new(Vec::new()));

    // Iterate indefinitely over all generated NetEvent until NodeHandler::stop() is called.
    listener.for_each(move |event| {
        match event.network() {
            NetEvent::Connected(_, _) => unreachable!(), // Used for explicit connections.
            NetEvent::Accepted(_endpoint, _listener) => info!("Client connected"),
            NetEvent::Message(_, data) => handle_input_message(
                &ui_display_image_handle,
                &render_busy_indicator,
                &sensor_value_history,
                data,
            ),
            NetEvent::Disconnected(_endpoint) => info!("Client disconnected"),
        }
    });
}

fn handle_input_message(
    ui_display_image_handle: &Arc<Mutex<Option<RetainedImage>>>,
    render_busy_indicator: &Arc<Mutex<bool>>,
    sensor_value_history: &Arc<Mutex<Vec<Vec<SensorValue>>>>,
    data: &[u8],
) {
    let transport_message: TransportMessage = bincode::deserialize(data).unwrap();
    let transport_type = transport_message.transport_type;
    let transport_data = transport_message.data;

    match transport_type {
        TransportType::PrepareStaticImage => {
            let prep_data: PrepareStaticImageData =
                bincode::deserialize(transport_data.as_slice()).unwrap();

            prepare_static_images(prep_data.images_data);
        }
        TransportType::PrepareConditionalImage => {
            let prep_data: PrepareConditionalImageData =
                bincode::deserialize(transport_data.as_slice()).unwrap();

            prepare_conditional_images(prep_data.images_data);
        }
        TransportType::RenderImage => {
            // If already rendering, skip this frame
            if *render_busy_indicator.lock().unwrap() {
                warn!(
                    "Received new sensor data, but rendering is still in progress, skipping frame!"
                );
                return;
            }

            let render_busy_indicator = render_busy_indicator.clone();
            let ui_display_image_handle = ui_display_image_handle.clone();
            let sensor_value_history = sensor_value_history.clone();

            thread::spawn(move || {
                // Begin rendering
                *render_busy_indicator.lock().unwrap() = true;

                let render_data: RenderData =
                    bincode::deserialize(transport_data.as_slice()).unwrap();

                renderer::render_image(
                    &ui_display_image_handle,
                    &sensor_value_history,
                    render_data,
                );

                // End rendering
                *render_busy_indicator.lock().unwrap() = false;
            });
        }
    }
}

/// Prepare static images for rendering.
/// This is done by storing each asset with its element id in the data folder on the filesystem
fn prepare_static_images(assets: HashMap<String, Vec<u8>>) {
    // Ensure data folder exists and is empty
    assets.par_iter().for_each(|(element_id, asset_data)| {
        let element_cache_dir = sensor_core::get_cache_dir(element_id, ElementType::StaticImage);
        let image_file_path = element_cache_dir.join(element_id);

        // Ensure cache dir exists and is empty
        std::fs::remove_dir_all(&element_cache_dir).unwrap_or_default();
        std::fs::create_dir_all(&element_cache_dir).unwrap();

        std::fs::write(image_file_path, asset_data).unwrap();
    });
}

/// Prepare conditional images for rendering.
/// This is done by storing each asset with its element id in the data folder on the filesystem
fn prepare_conditional_images(assets: HashMap<String, HashMap<String, Vec<u8>>>) {
    assets.par_iter().for_each(|element| {
        let element_id = element.0;
        let element_cache_dir =
            sensor_core::get_cache_dir(element_id, ElementType::ConditionalImage);

        // Ensure cache dir exists and is empty
        std::fs::remove_dir_all(&element_cache_dir).unwrap_or_default();
        std::fs::create_dir_all(&element_cache_dir).unwrap();

        element.1.par_iter().for_each(|asset| {
            let file_path = element_cache_dir.join(asset.0);
            let file_data = asset.1;
            std::fs::write(file_path, file_data).unwrap();
        })
    })
}
