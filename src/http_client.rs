use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use local_ip_address::local_ip;
use log::{error, info, warn};
use mac_address::get_mac_address;
use sensor_core::{ElementType, RenderData, SensorValue};
use serde::{Deserialize, Serialize};

use crate::ignore_poison_lock::LockResultExt;
use crate::{renderer, SharedImageHandle};

use rayon::prelude::*;

const DEFAULT_SERVER_PORT: u16 = 8080;
const POLL_INTERVAL_MS: u64 = 1000;

/// Static client data received from registration endpoint
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StaticClientData {
    /// Font data: font family name -> font bytes
    pub text_data: HashMap<String, Vec<u8>>,
    /// Static images: element ID -> PNG image bytes
    pub static_image_data: HashMap<String, Vec<u8>>,
    /// Conditional images: element ID -> (image name -> PNG image bytes)
    pub conditional_image_data: HashMap<String, HashMap<String, Vec<u8>>>,
}

/// Registration result containing processed preparation data
#[derive(Debug)]
pub struct RegistrationResult {
    pub success: bool,
    pub message: String,
    pub text_data: HashMap<String, Vec<u8>>,
    pub static_image_data: HashMap<String, Vec<u8>>,
    pub conditional_image_data: HashMap<String, HashMap<String, Vec<u8>>>,
}

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

        // Normalize MAC address to match server format (lowercase with colons)
        let normalized_mac = mac_address.to_lowercase();

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
    pub fn register(
        &self,
        name: Option<String>,
    ) -> Result<RegistrationResult, Box<dyn std::error::Error + Send + Sync>> {
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

        // Check if response indicates an error (4xx, 5xx status codes)
        let status_code = response.status();
        if status_code >= 400 {
            // Try to parse error response as JSON
            let error_result: Result<serde_json::Value, _> = response.into_json();
            match error_result {
                Ok(error_data) => {
                    let error_msg = error_data
                        .get("error")
                        .or_else(|| error_data.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    return Err(format!("Registration failed: {error_msg}").into());
                }
                Err(_) => {
                    return Err(format!("Registration failed with status: {status_code}").into());
                }
            }
        }

        // Success - process binary static data
        let mut binary_data = Vec::new();
        response.into_reader().read_to_end(&mut binary_data)?;

        info!(
            "Registration successful, received {} bytes of static data",
            binary_data.len()
        );

        // Process the binary data containing StaticClientData struct
        let result = self.process_static_preparation_data(&binary_data)?;

        Ok(result)
    }

    /// Process static preparation data from binary response
    fn process_static_preparation_data(
        &self,
        binary_data: &[u8],
    ) -> Result<RegistrationResult, Box<dyn std::error::Error + Send + Sync>> {
        // Deserialize the single StaticClientData struct from binary data
        let static_data: StaticClientData = bincode::deserialize(binary_data)?;

        info!("Processing static client data:");
        info!("  - {} font families", static_data.text_data.len());
        info!("  - {} static images", static_data.static_image_data.len());
        info!(
            "  - {} conditional image elements",
            static_data.conditional_image_data.len()
        );

        Ok(RegistrationResult {
            success: true,
            message: "Client registered successfully with static data".to_string(),
            text_data: static_data.text_data,
            static_image_data: static_data.static_image_data,
            conditional_image_data: static_data.conditional_image_data,
        })
    }

    /// Get sensor data from the server
    pub fn get_sensor_data(
        &self,
    ) -> Result<SensorDataResponse, Box<dyn std::error::Error + Send + Sync>> {
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
    server_host: String,
    server_port: Option<u16>,
    resolution: (u32, u32),
) {
    let render_busy_indicator = Arc::new(Mutex::new(false));
    let sensor_value_history: Arc<Mutex<Vec<Vec<SensorValue>>>> = Arc::new(Mutex::new(Vec::new()));
    let fonts_data: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));
    let static_image_data: Arc<Mutex<HashMap<String, Vec<u8>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let conditional_image_data: Arc<Mutex<HashMap<String, HashMap<String, Vec<u8>>>>> =
        Arc::new(Mutex::new(HashMap::new()));

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
                Ok(registration_result) => {
                    // Store the static data from registration
                    *fonts_data.lock().ignore_poison() = registration_result.text_data;
                    *static_image_data.lock().ignore_poison() =
                        registration_result.static_image_data;
                    *conditional_image_data.lock().ignore_poison() =
                        registration_result.conditional_image_data;

                    registered = true;
                    info!("Successfully registered with server and loaded static data");
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

                    // Process the render data with static data
                    handle_render_data(
                        &ui_display_image_handle,
                        &render_busy_indicator,
                        &sensor_value_history,
                        &fonts_data,
                        &static_image_data,
                        &conditional_image_data,
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
    static_image_data: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
    conditional_image_data: &Arc<Mutex<HashMap<String, HashMap<String, Vec<u8>>>>>,
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
    let static_image_data = static_image_data.clone();
    let conditional_image_data = conditional_image_data.clone();

    prepare_static_data(
        static_image_data.lock().ignore_poison().clone(),
        ElementType::StaticImage,
    );

    prepare_conditional_images(conditional_image_data.lock().ignore_poison().clone());

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
