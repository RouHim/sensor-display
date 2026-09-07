use std::error;
use std::io::Read;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use local_ip_address::local_ip;
use log::{error, info, warn};
use mac_address::get_mac_address;
use sensor_core::{RenderData, SensorValue, StaticClientData};
use serde::{Deserialize, Serialize};

use crate::{renderer, static_data, ui::SharedImageHandle};

const DEFAULT_SERVER_PORT: u16 = 55555;
const POLL_INTERVAL_MS: u64 = 1000;

/// Client registration request payload
#[derive(Serialize, Debug)]
pub struct ClientRegistrationRequestData {
    pub mac_address: String,
    pub ip_address: String,
    pub resolution_width: u16,
    pub resolution_height: u16,
    pub name: Option<String>,
}

/// Sensor data response from server
#[derive(Deserialize, Debug)]
pub struct SensorDataResponse {
    pub render_data: RenderData,
    pub static_data_reload_required: bool,
}

/// HTTP client for communicating with sensor bridge server
pub struct SensorBridgeClient {
    agent: ureq::Agent,
    server_url: String,
    mac_address: String,
    ip_address: String,
    resolution_width: u16,
    resolution_height: u16,
}

impl SensorBridgeClient {
    pub fn new(
        server_host: &str,
        server_port: Option<u16>,
        resolution: (u16, u16),
    ) -> Result<Self, Box<dyn error::Error + Send + Sync>> {
        let port = server_port.unwrap_or(DEFAULT_SERVER_PORT);
        #[allow(clippy::insecure_network_protocol)]
        let server_url = format!("http://{server_host}:{port}");

        let mac_address = get_mac_address()?
            .ok_or("Failed to get MAC address")?
            .to_string();

        // Normalize MAC address to match server format (lowercase with colons)
        let normalized_mac = mac_address.to_uppercase();

        let ip_address = local_ip()
            .map_err(|e| format!("Failed to get local IP: {e}"))?
            .to_string();

        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build();

        Ok(Self {
            agent,
            server_url,
            mac_address: normalized_mac,
            ip_address,
            resolution_width: resolution.0,
            resolution_height: resolution.1,
        })
    }

    /// Register with the sensor bridge server
    pub fn register(&self) -> Result<(), Box<dyn error::Error + Send + Sync>> {
        let registration_data = ClientRegistrationRequestData {
            mac_address: self.mac_address.clone(),
            ip_address: self.ip_address.clone(),
            resolution_width: self.resolution_width,
            resolution_height: self.resolution_height,
            name: None,
        };

        info!("Registering client with MAC: {}", self.mac_address);

        let response = self
            .agent
            .post(&format!("{}/api/register", self.server_url))
            .send_json(&registration_data)?;

        // Check if response indicates an error (4xx, 5xx status codes)
        let status_code = response.status();
        if status_code >= 400 {
            // Try to parse error response as JSON
            let error_result: Result<serde_json::Value, _> = response.into_json();
            return match error_result {
                Ok(error_data) => {
                    let error_msg = error_data
                        .get("error")
                        .or_else(|| error_data.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    Err(format!("Registration failed: {error_msg}").into())
                }
                Err(_) => Err(format!("Registration failed with status: {status_code}").into()),
            };
        }

        // Success - parse JSON response
        let result: serde_json::Value = response.into_json()?;

        if result["success"] == true {
            info!("Registration successful");
            Ok(())
        } else {
            Err("Registration failed".into())
        }
    }

    /// Get static data from the server
    pub fn get_static_data(
        &self,
    ) -> Result<static_data::StaticDataResult, Box<dyn error::Error + Send + Sync>> {
        let url = format!(
            "{}/api/static-data?mac_address={}",
            self.server_url, self.mac_address
        );

        let response = self.agent.get(&url).call()?;

        // Check if response indicates an error (4xx, 5xx status codes)
        let status_code = response.status();
        if status_code >= 400 {
            // Try to parse error response as JSON
            let error_result: Result<serde_json::Value, _> = response.into_json();
            return match error_result {
                Ok(error_data) => {
                    let error_msg = error_data
                        .get("error")
                        .or_else(|| error_data.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    Err(format!("Failed to get static data: {error_msg}").into())
                }
                Err(_) => {
                    Err(format!("Failed to get static data with status: {status_code}").into())
                }
            };
        }

        // Success - process binary static data
        let mut binary_data = Vec::new();
        response.into_reader().read_to_end(&mut binary_data)?;

        info!("Static data received, {} bytes", binary_data.len());

        // Process the binary data containing StaticClientData struct
        let result = self.process_static_data(&binary_data)?;

        Ok(result)
    }

    /// Process static data from binary response
    fn process_static_data(
        &self,
        binary_data: &[u8],
    ) -> Result<static_data::StaticDataResult, Box<dyn error::Error + Send + Sync>> {
        // Deserialize the single StaticClientData struct from binary data
        let static_client_data: StaticClientData = bincode::deserialize(binary_data)?;

        info!("Processing static client data:");
        info!("  - {} font families", static_client_data.text_data.len());
        info!(
            "  - {} static images",
            static_client_data.static_image_data.len()
        );
        info!(
            "  - {} conditional image elements",
            static_client_data.conditional_image_data.len()
        );

        Ok(static_data::StaticDataResult {
            text_data: static_client_data.text_data,
            static_image_data: static_client_data.static_image_data,
            conditional_image_data: static_client_data.conditional_image_data,
        })
    }

    /// Get sensor data from the server
    pub fn get_sensor_data(
        &self,
    ) -> Result<SensorDataResponse, Box<dyn error::Error + Send + Sync>> {
        let url = format!(
            "{}/api/sensor-data?mac_address={}",
            self.server_url, self.mac_address
        );

        let response = self.agent.get(&url).call();

        match response {
            Ok(resp) => match resp.into_json::<SensorDataResponse>() {
                Ok(data) => Ok(data),
                Err(err) => {
                    error!("Failed to parse sensor data response: {err}");
                    Err(err.into())
                }
            },
            Err(ureq::Error::Status(404, _)) => Err("Client not registered".into()),
            Err(ureq::Error::Status(403, _)) => Err("Client not active".into()),
            Err(e) => Err(format!("Failed to get sensor data: {e}").into()),
        }
    }
}

/// Start HTTP client and begin polling for sensor data
pub fn start_http_client(
    ui_display_image_handle: SharedImageHandle,
    font_cache: Arc<RwLock<lru::LruCache<String, rusttype::Font<'static>>>>,
    server_host: String,
    server_port: Option<u16>,
    resolution: (u16, u16),
) {
    let render_busy_indicator = Arc::new(RwLock::new(false));
    let sensor_value_history: Arc<RwLock<Vec<Vec<SensorValue>>>> =
        Arc::new(RwLock::new(Vec::new()));

    std::thread::spawn(move || {
        let client = match SensorBridgeClient::new(&server_host, server_port, resolution) {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to create HTTP client: {e}");
                return;
            }
        };

        // Initial registration and static data loading
        let mut has_static_data = false;
        while !has_static_data {
            // First, register with the server
            match client.register() {
                Ok(()) => {
                    info!("Registration successful, now getting initial static data");

                    // Then get initial static data
                    match client.get_static_data() {
                        Ok(static_data_result) => {
                            // Persist static data to disk
                            if let Err(e) =
                                static_data::persist_static_data_to_disk(&static_data_result)
                            {
                                error!("Failed to persist initial static data: {}", e);
                                std::thread::sleep(Duration::from_secs(5));
                                continue;
                            }

                            has_static_data = true;
                            info!("Successfully registered with server and loaded initial static data");
                        }
                        Err(e) => {
                            error!(
                                "Failed to get initial static data: {e}. Retrying in 5 seconds..."
                            );
                            std::thread::sleep(Duration::from_secs(5));
                        }
                    }
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

                    // Check if static data reload is required
                    if response.static_data_reload_required {
                        info!("Static data reload required, fetching updated static data");
                        match client.get_static_data() {
                            Ok(static_data_result) => {
                                if let Err(e) =
                                    static_data::persist_static_data_to_disk(&static_data_result)
                                {
                                    error!("Failed to persist updated static data: {}", e);
                                } else {
                                    info!("Static data reloaded successfully due to configuration change");
                                }
                            }
                            Err(e) => {
                                error!("Failed to reload static data: {}", e);
                            }
                        }
                    }

                    // Process the render data
                    handle_render_data(
                        &ui_display_image_handle,
                        &render_busy_indicator,
                        &sensor_value_history,
                        &font_cache,
                        response.render_data,
                        client.resolution_width,
                        client.resolution_height,
                    );
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("not active") {
                        warn!("Client is not active. Please activate in the server UI.");
                    } else if error_msg.contains("not registered") {
                        warn!("Client not registered. Re-registering...");
                        if let Err(reg_err) = client.register() {
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

/// Handle render data received from the server - now uses filesystem cache
fn handle_render_data(
    ui_display_image_handle: &SharedImageHandle,
    render_busy_indicator: &Arc<RwLock<bool>>,
    sensor_value_history: &Arc<RwLock<Vec<Vec<SensorValue>>>>,
    font_cache: &Arc<RwLock<lru::LruCache<String, rusttype::Font<'static>>>>,
    render_data: RenderData,
    image_width: u16,
    image_height: u16,
) {
    // If already rendering, skip this frame
    if *render_busy_indicator.read().unwrap() {
        warn!("Received new sensor data, but rendering is still in progress, skipping frame!");
        return;
    }

    let render_busy_indicator = render_busy_indicator.clone();
    let ui_display_image_handle = ui_display_image_handle.clone();
    let sensor_value_history = sensor_value_history.clone();
    let font_cache = Arc::clone(font_cache);

    // Spawn blocking task for rendering (since renderer is not async)
    std::thread::spawn(move || {
        // Begin rendering
        *render_busy_indicator.write().unwrap() = true;

        // Define render closure
        let do_render = || -> Result<(), Box<dyn error::Error>> {
            renderer::render_image(
                &ui_display_image_handle,
                &sensor_value_history,
                &font_cache,
                render_data,
                image_width,
                image_height,
            );
            Ok(())
        };

        // Render image
        if let Err(e) = do_render() {
            error!("Error while rendering image: {e:?}");
        }

        // End rendering
        *render_busy_indicator.write().unwrap() = false;
    });
}

/// Get local IP address for registration
pub fn get_local_ip_address() -> Vec<String> {
    match local_ip() {
        Ok(ip) => vec![ip.to_string()],
        Err(_) => vec!["127.0.0.1".to_string()],
    }
}
