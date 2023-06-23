use std::io::Read;
use std::time::Duration;
use serialport::*;

fn main() {
    let port_name = "/dev/ttyUSB0";
    let mut port = serialport::new(port_name, 12000)
        .timeout(Duration::from_millis(100))
        .open()
        .expect("Failed to open port");

    let mut serial_buf: Vec<u8> = vec![0; 100];
    loop {
        match port.read(serial_buf.as_mut_slice()) {
            Ok(t) => {
                println!("Read {} bytes from serial port", t);
                println!("{:?}", serial_buf);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
            Err(e) => eprintln!("{:?}", e),
        }
    }

}
