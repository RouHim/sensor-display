use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use local_ip_address::local_ip;
use log::{error, info, warn};
use mac_address::get_mac_address;
use sensor_core::{RenderData, SensorValue};
use serde::{Deserialize, Serialize};

use crate::ignore_poison_lock::LockResultExt;
use crate::{renderer, SharedImageHandle};

const DEFAULT_SERVER_PORT: u16 = 8080;
const POLL_INTERVAL_MS: u64 = 1000;

/// Client registration request payload
#[derive(Serialize, Debug)]
pub struct ClientRegistration {
    pub mac_address: String,
    pub ip_address: String,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub name: Option<String>,
}

/// Client registration response
#[derive(Deserialize, Debug)]
pub struct RegistrationResponse {
    pub success: bool,
    pub message: String,
    pub client: Option<RegisteredClient>,
}

/// Registered client information
#[derive(Deserialize, Debug)]
pub struct RegisteredClient {
    pub mac_address: String,
    pub name: String,
    pub ip_address: String,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub active: bool,
    pub last_seen: u64,
    pub display_config: sensor_core::DisplayConfig,
}

/// Sensor data response from server
#[derive(Deserialize, Debug)]
pub struct SensorDataResponse {
    pub render_data: RenderData,
    pub timestamp: u64,
}

/// HTTP client for communicating with sensor bridge server
pub struct SensorBridgeClient {
    agent: ureq::Agent,
    server_url: String,
    mac_address: String,
    ip_address: String,
    resolution_width: u32,
    resolution_height: u32,
}

impl SensorBridgeClient {
    pub fn new(
        server_host: &str,
        server_port: Option<u16>,
        resolution: (u32, u32),
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let port = server_port.unwrap_or(DEFAULT_SERVER_PORT);
        let server_url = format!("http://{server_host}:{port}");

        let mac_address = get_mac_address()?
            .ok_or("Failed to get MAC address")?
            .to_string();

        let ip_address = local_ip()
            .map_err(|e| format!("Failed to get local IP: {e}"))?
            .to_string();

        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build();

        Ok(Self {
            agent,
            server_url,
            mac_address,
            ip_address,
            resolution_width: resolution.0,
            resolution_height: resolution.1,
        })
    }

    /// Register with the sensor bridge server
    pub fn register(
        &self,
        name: Option<String>,
    ) -> Result<RegistrationResponse, Box<dyn std::error::Error + Send + Sync>> {
        let registration_data = ClientRegistration {
            mac_address: self.mac_address.clone(),
            ip_address: self.ip_address.clone(),
            resolution_width: self.resolution_width,
            resolution_height: self.resolution_height,
            name,
        };

        info!("Registering client with MAC: {}", self.mac_address);

        let response = self
            .agent
            .post(&format!("{}/api/register", self.server_url))
            .send_json(&registration_data)?;

        let result: RegistrationResponse = response.into_json()?;
        info!("Registration successful: {}", result.message);
        Ok(result)
    }

    /// Get sensor data from the server
    pub fn get_sensor_data(
        &self,
    ) -> Result<SensorDataResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/sensors", self.server_url);

        let response = self.agent.get(&url).call();

        match response {
            Ok(resp) => {
                let data: SensorDataResponse = resp.into_json()?;
                Ok(data)
            }
            Err(ureq::Error::Status(404, _)) => Err("Client not registered".into()),
            Err(ureq::Error::Status(403, _)) => Err("Client not active".into()),
            Err(e) => Err(format!("Failed to get sensor data: {e}").into()),
        }
    }

    /// Check server health
    pub fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let response = self
            .agent
            .get(&format!("{}/api/health", self.server_url))
            .call();

        match response {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Start HTTP client and begin polling for sensor data
pub fn start_http_client(
    ui_display_image_handle: SharedImageHandle,
    server_host: String,
    server_port: Option<u16>,
    resolution: (u32, u32),
) {
    let render_busy_indicator = Arc::new(Mutex::new(false));
    let sensor_value_history: Arc<Mutex<Vec<Vec<SensorValue>>>> = Arc::new(Mutex::new(Vec::new()));
    let fonts_data: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    std::thread::spawn(move || {
        let client = match SensorBridgeClient::new(&server_host, server_port, resolution) {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to create HTTP client: {e}");
                return;
            }
        };

        // Initial registration
        let mut registered = false;
        while !registered {
            match client.register(None) {
                Ok(_) => {
                    registered = true;
                    info!("Successfully registered with server");
                }
                Err(e) => {
                    error!("Registration failed: {e}. Retrying in 5 seconds...");
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }

        info!("Starting sensor data polling loop");
        info!("Note: Client must be activated in the server UI to receive data");

        // Main polling loop
        loop {
            match client.get_sensor_data() {
                Ok(response) => {
                    info!(
                        "Received sensor data with {} sensor values",
                        response.render_data.sensor_values.len()
                    );

                    // Process the render data
                    handle_render_data(
                        &ui_display_image_handle,
                        &render_busy_indicator,
                        &sensor_value_history,
                        &fonts_data,
                        response.render_data,
                    );
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("not active") {
                        warn!("Client is not active. Please activate in the server UI.");
                    } else if error_msg.contains("not registered") {
                        warn!("Client not registered. Re-registering...");
                        if let Err(reg_err) = client.register(None) {
                            error!("Re-registration failed: {reg_err}");
                        }
                    } else {
                        error!("Error polling sensor data: {e}");
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    });
}

/// Handle render data received from the server
fn handle_render_data(
    ui_display_image_handle: &SharedImageHandle,
    render_busy_indicator: &Arc<Mutex<bool>>,
    sensor_value_history: &Arc<Mutex<Vec<Vec<SensorValue>>>>,
    fonts_data: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
    render_data: RenderData,
) {
    // If already rendering, skip this frame
    if *render_busy_indicator.lock().ignore_poison() {
        warn!("Received new sensor data, but rendering is still in progress, skipping frame!");
        return;
    }

    let render_busy_indicator = render_busy_indicator.clone();
    let ui_display_image_handle = ui_display_image_handle.clone();
    let sensor_value_history = sensor_value_history.clone();
    let fonts_data = fonts_data.clone();

    // Spawn blocking task for rendering (since renderer is not async)
    std::thread::spawn(move || {
        // Begin rendering
        *render_busy_indicator.lock().unwrap() = true;

        // Define render closure, so if something in the render process goes wrong, we can
        // still end the render process and set the render_busy_indicator to false
        let do_render = || -> Result<(), Box<dyn std::error::Error>> {
            renderer::render_image(
                &ui_display_image_handle,
                &sensor_value_history,
                render_data,
                &fonts_data,
            );
            Ok(())
        };

        // Render image
        if let Err(e) = do_render() {
            error!("Error while rendering image: {e:?}");
        }

        // End rendering
        *render_busy_indicator.lock().unwrap() = false;
    });
}

/// Get local IP address for registration
pub fn get_local_ip_address() -> Vec<String> {
    match local_ip() {
        Ok(ip) => vec![ip.to_string()],
        Err(_) => vec!["127.0.0.1".to_string()],
    }
}
