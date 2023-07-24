use egui_extras::RetainedImage;
use image::ImageOutputFormat;
use message_io::network::{NetEvent, Transport};
use message_io::node::{self, NodeHandler, NodeListener};
use sensor_core::TransferData;
use std::io::{Cursor, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

const PORT: u16 = 10489;

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
    write_image_data_mutex: Arc<Mutex<Option<RetainedImage>>>,
    listener: NodeListener<()>,
) {
    listener.for_each(move |event| {
        match event.network() {
            NetEvent::Connected(_, _) => unreachable!(), // Used for explicit connections.
            NetEvent::Accepted(_endpoint, _listener) => println!("Client connected"),
            NetEvent::Message(_, data) => render_image(&write_image_data_mutex, data),
            NetEvent::Disconnected(_endpoint) => println!("Client disconnected"),
        }
    });
}

fn render_image(write_image_data_mutex: &Arc<Mutex<Option<RetainedImage>>>, data: &[u8]) {
    println!("Received data with length {}", data.len());

    // Measure deserialization and rendering time
    let start = std::time::Instant::now();

    let data: TransferData = deserialize(data);
    let image_data = sensor_core::render_lcd_image(data.lcd_config, data.sensor_values);

    let lcd_render_time = std::time::Instant::now();
    println!(
        "Deserialization and rendering took {} ms",
        lcd_render_time.duration_since(start).as_millis()
    );

    // Create a Vec<u8> buffer to write the image to it
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    image_data
        .write_to(&mut cursor, ImageOutputFormat::Png)
        .unwrap();

    // Reset the cursor to the beginning of the buffer
    cursor.seek(SeekFrom::Start(0)).unwrap();

    // Log time it took to write image to buffer
    let write_image_to_buffer_time = std::time::Instant::now();
    println!(
        "Writing image to buffer took {} ms",
        write_image_to_buffer_time
            .duration_since(lcd_render_time)
            .as_millis()
    );

    let image = RetainedImage::from_image_bytes("test", buf.as_slice()).unwrap();

    // Log time it took to create RetainedImage
    let create_retained_image_time = std::time::Instant::now();
    println!(
        "Creating RetainedImage took {} ms",
        create_retained_image_time
            .duration_since(write_image_to_buffer_time)
            .as_millis()
    );

    // Write image data to mutex
    let mut mutex = write_image_data_mutex.lock().unwrap();
    *mutex = Some(image);
}

fn deserialize(data: &[u8]) -> TransferData {
    serde::Deserialize::deserialize(&mut rmp_serde::Deserializer::new(data)).unwrap()
}
