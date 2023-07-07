use message_io::network::{NetEvent, Transport};
use message_io::node::{self};
use sensor_core::TransferData;

fn main() {
    let (handler, listener) = node::split::<()>();

    // Listen for TCP, UDP and WebSocket messages at the same time.
    handler
        .network()
        .listen(Transport::FramedTcp, "0.0.0.0:10489")
        .unwrap();

    // Read incoming network events.
    listener.for_each(move |event| match event.network() {
        NetEvent::Connected(_, _) => unreachable!(), // Used for explicit connections.
        NetEvent::Accepted(_endpoint, _listener) => println!("Client connected"),
        NetEvent::Message(_, data) => {
            let data: TransferData = deserialize(data);
        }
        NetEvent::Disconnected(_endpoint) => println!("Client disconnected"),
    });
}

fn deserialize(data: &[u8]) -> TransferData {
    serde::Deserialize::deserialize(&mut rmp_serde::Deserializer::new(data)).unwrap()
}
