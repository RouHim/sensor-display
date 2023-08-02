use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::renderer;
use egui_extras::RetainedImage;
use log::{info, warn};
use message_io::network::{NetEvent, Transport};
use message_io::node::{self, NodeHandler, NodeListener};
use rayon::prelude::*;
use sensor_core::{AssetData, RenderData, TransportMessage, TransportType};

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

    // Iterate indefinitely over all generated NetEvent until NodeHandler::stop() is called.
    listener.for_each(move |event| {
        match event.network() {
            NetEvent::Connected(_, _) => unreachable!(), // Used for explicit connections.
            NetEvent::Accepted(_endpoint, _listener) => info!("Client connected"),
            NetEvent::Message(_, data) => {
                handle_input_message(&ui_display_image_handle, &render_busy_indicator, data)
            }
            NetEvent::Disconnected(_endpoint) => info!("Client disconnected"),
        }
    });
}

fn handle_input_message(
    ui_display_image_handle: &Arc<Mutex<Option<RetainedImage>>>,
    render_busy_indicator: &Arc<Mutex<bool>>,
    data: &[u8],
) {
    let transport_message: TransportMessage =
        serde::Deserialize::deserialize(&mut rmp_serde::Deserializer::new(data)).unwrap();
    let transport_type = transport_message.transport_type;
    let transport_data = transport_message.data;

    match transport_type {
        TransportType::PrepareData => {
            let asset_data: AssetData = serde::Deserialize::deserialize(
                &mut rmp_serde::Deserializer::new(transport_data.as_slice()),
            )
            .unwrap();
            prepare_assets(asset_data.asset_data);
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

            thread::spawn(move || {
                // Begin rendering
                *render_busy_indicator.lock().unwrap() = true;

                let render_data: RenderData = serde::Deserialize::deserialize(
                    &mut rmp_serde::Deserializer::new(transport_data.as_slice()),
                )
                .unwrap();

                renderer::render_image(&ui_display_image_handle, render_data);

                // End rendering
                *render_busy_indicator.lock().unwrap() = false;
            });
        }
    }
}

/// Prepare assets for rendering.
/// This is done by storing each asset with its asset / element id in the data folder on the filesystem
fn prepare_assets(assets: HashMap<String, Vec<u8>>) {
    let start = std::time::Instant::now();

    // Ensure data folder exists
    std::fs::create_dir_all(sensor_core::ASSET_DATA_DIR).unwrap();

    assets.par_iter().for_each(|(asset_id, asset_data)| {
        let asset_path = format!("{}/{}", sensor_core::ASSET_DATA_DIR, asset_id);
        std::fs::write(asset_path, asset_data).unwrap();
    });

    info!("Prepared assets in {}ms", start.elapsed().as_millis());
}
