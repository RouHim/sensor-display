use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use local_ip_address::local_ip;
use log::{error, info, warn};
use message_io::network::{NetEvent, Transport};
use message_io::node::{self, NodeHandler, NodeListener};
use rayon::prelude::*;
use sensor_core::{
    ElementType, PrepareConditionalImageData, PrepareStaticImageData, PrepareTextData, RenderData,
    SensorValue, TransportMessage, TransportType,
};

use crate::ignore_poison_lock::LockResultExt;
use crate::{renderer, SharedImageHandle};

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

pub fn receive(ui_display_image_handle: SharedImageHandle, tcp_listener: NodeListener<()>) {
    let render_busy_indicator = Arc::new(Mutex::new(false));
    let sensor_value_history: Arc<Mutex<Vec<Vec<SensorValue>>>> = Arc::new(Mutex::new(Vec::new()));
    let fonts_data: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    // Iterate indefinitely over all generated NetEvent until NodeHandler::stop() is called.
    tcp_listener.for_each(move |event| {
        match event.network() {
            NetEvent::Connected(_, _) => unreachable!(), // Used for explicit connections.
            NetEvent::Accepted(_endpoint, _listener) => info!("Client connected"),
            NetEvent::Message(_, data) => handle_input_message(
                &ui_display_image_handle,
                &render_busy_indicator,
                &sensor_value_history,
                &fonts_data,
                data,
            ),
            NetEvent::Disconnected(_endpoint) => info!("Client disconnected"),
        }
    });
}

fn handle_input_message(
    ui_display_image_handle: &SharedImageHandle,
    render_busy_indicator: &Arc<Mutex<bool>>,
    sensor_value_history: &Arc<Mutex<Vec<Vec<SensorValue>>>>,
    fonts_data: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
    data: &[u8],
) {
    info!("Received new sensor data: {:?} Bytes", data.len());

    let transport_message: TransportMessage = bincode::deserialize(data).unwrap();
    let transport_type = transport_message.transport_type;
    let transport_data = transport_message.data;
    let fonts_data = fonts_data.clone();

    info!("Type: {:?}", transport_type);

    match transport_type {
        TransportType::PrepareText => {
            let prep_data: PrepareTextData =
                bincode::deserialize(transport_data.as_slice()).unwrap();

            let mut font_data_lock = fonts_data.lock().ignore_poison();
            font_data_lock.clear();
            font_data_lock.extend(prep_data.font_data);
        }
        TransportType::PrepareStaticImage => {
            let prep_data: PrepareStaticImageData =
                bincode::deserialize(transport_data.as_slice()).unwrap();
            prepare_static_data(prep_data.images_data, ElementType::StaticImage);
        }
        TransportType::PrepareConditionalImage => {
            let prep_data: PrepareConditionalImageData =
                bincode::deserialize(transport_data.as_slice()).unwrap();
            prepare_conditional_images(prep_data.images_data);
        }
        TransportType::RenderImage => {
            // If already rendering, skip this frame
            if *render_busy_indicator.lock().ignore_poison() {
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

                let result = std::panic::catch_unwind(|| {
                    let render_data: RenderData =
                        bincode::deserialize(transport_data.as_slice()).unwrap();

                    renderer::render_image(
                        &ui_display_image_handle,
                        &sensor_value_history,
                        render_data,
                        &fonts_data,
                    );
                });

                // End rendering
                *render_busy_indicator.lock().unwrap() = false;

                if let Err(err) = result {
                    error!("Error while rendering: {:?}", err);
                }
            });
        }
    }
}

/// Prepare static data for rendering on the local filesystem.
/// This is done by storing each asset with its element id in the data folder on the filesystem
/// /// # Parameters
// /// * `assets` - A hashmap containing the data for each element
fn prepare_static_data(assets: HashMap<String, Vec<u8>>, element_type: ElementType) {
    // Ensure data folder exists and is empty
    assets.par_iter().for_each(|(element_id, asset_data)| {
        let element_cache_dir = sensor_core::get_cache_dir(element_id, &element_type);
        let file_path = element_cache_dir.join(element_id);

        // Ensure cache dir exists and is empty
        std::fs::remove_dir_all(&element_cache_dir).unwrap_or_default();
        std::fs::create_dir_all(&element_cache_dir).unwrap();

        std::fs::write(file_path, asset_data).unwrap();
    });
}

/// Prepare conditional images for rendering.
/// This is done by storing each asset with its element id in the data folder on the filesystem
/// # Parameters
/// * `assets` - A hashmap containing the image data for each conditional image element
fn prepare_conditional_images(assets: HashMap<String, HashMap<String, Vec<u8>>>) {
    assets.par_iter().for_each(|element| {
        let element_id = element.0;
        let element_cache_dir =
            sensor_core::get_cache_dir(element_id, &ElementType::ConditionalImage);

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

pub fn get_local_ip_address() -> Vec<String> {
    if let Ok(my_local_ip) = local_ip() {
        vec![my_local_ip.to_string()]
    } else {
        vec![]
    }
}
