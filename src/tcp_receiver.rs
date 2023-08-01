use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui_extras::RetainedImage;
use log::info;
use message_io::network::{NetEvent, Transport};
use message_io::node::{self, NodeHandler, NodeListener};
use sensor_core::{AssetData, RenderData, TransportMessage, TransportType};

use crate::renderer::render_image;

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
    listener.for_each(move |event| {
        match event.network() {
            NetEvent::Connected(_, _) => unreachable!(), // Used for explicit connections.
            NetEvent::Accepted(_endpoint, _listener) => info!("Client connected"),
            NetEvent::Message(_, data) => handle_input_message(&ui_display_image_handle, data),
            NetEvent::Disconnected(_endpoint) => info!("Client disconnected"),
        }
    });
}

fn handle_input_message(ui_display_image_handle: &Arc<Mutex<Option<RetainedImage>>>, data: &[u8]) {
    let transport_message: TransportMessage =
        serde::Deserialize::deserialize(&mut rmp_serde::Deserializer::new(data)).unwrap();
    let transport_type = transport_message.transport_type;
    let transport_data = transport_message.data.as_slice();

    match transport_type {
        TransportType::PrepareData => {
            let asset_data: AssetData =
                serde::Deserialize::deserialize(&mut rmp_serde::Deserializer::new(transport_data))
                    .unwrap();
            prepare_assets(asset_data.asset_data);
        }
        TransportType::RenderImage => {
            let render_data: RenderData =
                serde::Deserialize::deserialize(&mut rmp_serde::Deserializer::new(transport_data))
                    .unwrap();
            render_image(ui_display_image_handle, render_data);
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
