# Sensor Display

Sensor Display is part of a two-application system that allows you to display sensor information from one device on
another device's screen. The other part of the system is the [Sensor Bridge](https://github.com/RouHim/sensor-bridge)
application which collects the sensor data.

## Current State

This software is still in development. Expect bugs and unresolved issues.

## Features

* Connects to the Sensor Bridge server via HTTP to receive sensor data and display configurations
* Automatically registers with the server using MAC address identification
* Displays real-time sensor information on the screen with configurable layouts
* Reduces memory and CPU consumption on the device collecting the data, as rendering is offloaded to the device running
  Sensor Display
* Supports fullscreen display mode for dedicated monitor setups

## Motivation

This project was born out of the need to display sensor data (such as FPS, CPU load, GPU load, etc.) from a computer on
a separate display. Existing solutions either required payment, didn't support Linux, or rendered the display on the
computer collecting the data, thus consuming resources.

## Architecture

As mentioned before, this system requires two applications:

1. The Sensor Bridge application that runs on the device collecting the sensor data and serves an HTTP API.
2. The Sensor Display application that runs on a separate device (this could be another computer, a Raspberry Pi, or
   similar) with a connected display.

The sensor data is sent via HTTP from the device running Sensor Bridge to the device running Sensor Display, where it is then
displayed based on the configuration defined in the Sensor Bridge UI.

## Communication Protocol

The communication between Sensor Bridge and Sensor Display uses HTTP:

1. **Client Registration**: Sensor Display registers itself with the Sensor Bridge server by sending its MAC address, IP address, and display resolution
2. **Activation**: The client must be activated through the Sensor Bridge UI before it can receive data
3. **Data Polling**: Sensor Display polls the server every second for new sensor data and display configurations
4. **Real-time Rendering**: Received data is rendered immediately on the display

## Configuration

Sensor Display can be configured using environment variables:

- `SENSOR_BRIDGE_HOST`: The hostname or IP address of the Sensor Bridge server (default: `localhost`)
- `SENSOR_BRIDGE_PORT`: The port of the Sensor Bridge server (default: `8080`)

Example:
```bash
export SENSOR_BRIDGE_HOST=192.168.1.100
export SENSOR_BRIDGE_PORT=8080
./sensor-display
```

## Setup Instructions

### Prerequisites

- Rust toolchain (for building from source)
- A running Sensor Bridge server instance

### Building and Running

1. Clone the repository:
   ```bash
   git clone https://github.com/RouHim/sensor-display.git
   cd sensor-display
   ```

2. Build the application:
   ```bash
   cargo build --release
   ```

3. Set the server configuration (optional):
   ```bash
   export SENSOR_BRIDGE_HOST=your-server-ip
   export SENSOR_BRIDGE_PORT=8080
   ```

4. Run the application:
   ```bash
   cargo run --release
   ```

### First Time Setup

1. Start Sensor Display - it will automatically register with the Sensor Bridge server
2. Open the Sensor Bridge UI in your web browser
3. Navigate to the "Clients" section
4. Find your Sensor Display client (identified by MAC address) and activate it
5. Configure the display layout and elements in the Sensor Bridge UI
6. The display should start showing sensor data within a few seconds

## Troubleshooting

### Common Issues

**"No data received yet" message**
- Ensure the Sensor Bridge server is running and accessible
- Check that the client is activated in the Sensor Bridge UI
- Verify network connectivity between devices
- Check the server host/port configuration

**Client not appearing in Sensor Bridge UI**
- Check network connectivity
- Verify the server host/port configuration
- Check the Sensor Display logs for registration errors

**Display not updating**
- Ensure the client is activated in the Sensor Bridge UI
- Check that display elements are configured in the Sensor Bridge UI
- Verify sensor data is being collected on the server

### Logs

Enable detailed logging by setting the `RUST_LOG` environment variable:
```bash
export RUST_LOG=info
./sensor-display
```

For debug-level logging:
```bash
export RUST_LOG=debug
./sensor-display
```

## Network Requirements

- HTTP communication on port 8080 (or configured port)
- The Sensor Display device must be able to reach the Sensor Bridge server
- No incoming connections required on the Sensor Display device
