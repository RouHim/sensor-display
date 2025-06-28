use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use local_ip_address::local_ip;
use log::{error, info, warn};
use rayon::prelude::*;
use sensor_core::{
    DisplayConfig, ElementType, PrepareConditionalImageData, PrepareStaticImageData,
    PrepareTextData, RenderData, SensorValue, TransportMessage, TransportType,
};
use serde::{Deserialize, Serialize};
use ureq::Agent;

use crate::ignore_poison_lock::LockResultExt;
use crate::{renderer, SharedImageHandle};

const DEFAULT_SERVER_PORT: u16 = 10489;
const DISCOVERY_PORT: u16 = 10490;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceDiscoveryMessage {
    pub service_name: String,
    pub server_port: u16,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct DiscoveredServer {
    pub ip: String,
    pub port: u16,
    pub service_name: String,
    pub version: String,
}

impl DiscoveredServer {
    pub fn address(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ClientRegistration {
    name: String,
    display_config: DisplayConfig,
}

#[derive(Debug, Deserialize)]
struct RegistrationResponse {
    client_id: String,
    status: String,
}

pub struct HttpClientConfig {
    pub server_address: String,
    pub device_name: String,
    pub display_config: DisplayConfig,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            server_address: format!("127.0.0.1:{DEFAULT_SERVER_PORT}"),
            device_name: "sensor-display".to_string(),
            display_config: DisplayConfig {
                resolution_width: 128,
                resolution_height: 64,
                elements: Vec::new(),
            },
        }
    }
}

pub fn start_client(ui_display_image_handle: SharedImageHandle, config: Option<HttpClientConfig>) {
    let config = config.unwrap_or_default();

    thread::spawn(move || {
        run_client_loop(ui_display_image_handle, config);
    });
}

fn run_client_loop(ui_display_image_handle: SharedImageHandle, config: HttpClientConfig) {
    let agent = Agent::new();
    let render_busy_indicator = Arc::new(Mutex::new(false));
    let sensor_value_history: Arc<Mutex<Vec<Vec<SensorValue>>>> = Arc::new(Mutex::new(Vec::new()));
    let fonts_data: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    loop {
        match register_and_run(
            &agent,
            &config,
            &ui_display_image_handle,
            &render_busy_indicator,
            &sensor_value_history,
            &fonts_data,
        ) {
            Ok(_) => {
                info!("Client session ended normally");
            }
            Err(e) => {
                error!("Client error: {e:?}");
                warn!("Retrying in {} seconds...", RETRY_DELAY.as_secs());
                thread::sleep(RETRY_DELAY);
            }
        }
    }
}

fn register_and_run(
    agent: &Agent,
    config: &HttpClientConfig,
    ui_display_image_handle: &SharedImageHandle,
    render_busy_indicator: &Arc<Mutex<bool>>,
    sensor_value_history: &Arc<Mutex<Vec<Vec<SensorValue>>>>,
    fonts_data: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Test server connectivity
    let ping_url = format!("http://{}/ping", config.server_address);
    let ping_response = agent.get(&ping_url).call()?;
    let ping_text = ping_response.into_string()?;
    if ping_text != "pong" {
        return Err("Server ping failed".into());
    }
    info!("Server connectivity verified");

    // Register with the server
    let registration = ClientRegistration {
        name: config.device_name.clone(),
        display_config: config.display_config.clone(),
    };

    let register_url = format!("http://{}/register", config.server_address);
    let registration_response = agent.post(&register_url).send_json(&registration)?;

    let registration_result: RegistrationResponse = registration_response.into_json()?;
    let client_id = registration_result.client_id;

    info!("Successfully registered with server. Client ID: {client_id}");

    // Download static data
    download_static_data(agent, config, fonts_data)?;

    // Start data polling loop
    poll_data_loop(
        agent,
        config,
        &client_id,
        ui_display_image_handle,
        render_busy_indicator,
        sensor_value_history,
        fonts_data,
    )?;

    Ok(())
}

fn download_static_data(
    agent: &Agent,
    config: &HttpClientConfig,
    fonts_data: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Downloading static data...");

    // Download text preparation data (fonts)
    let text_url = format!(
        "http://{}/static/text_{}",
        config.server_address, config.device_name
    );
    match agent.get(&text_url).call() {
        Ok(response) => {
            let mut data = Vec::new();
            response.into_reader().read_to_end(&mut data)?;
            let prep_data: PrepareTextData = bincode::deserialize(&data)?;

            let mut font_data_lock = fonts_data.lock().ignore_poison();
            font_data_lock.clear();
            font_data_lock.extend(prep_data.font_data);
            info!("Downloaded text preparation data");
        }
        Err(e) => {
            warn!("Failed to download text data: {e:?}");
        }
    }

    // Download static image data
    let static_image_url = format!(
        "http://{}/static/static_image_{}",
        config.server_address, config.device_name
    );
    match agent.get(&static_image_url).call() {
        Ok(response) => {
            let mut data = Vec::new();
            response.into_reader().read_to_end(&mut data)?;
            let prep_data: PrepareStaticImageData = bincode::deserialize(&data)?;
            prepare_static_data(prep_data.images_data, ElementType::StaticImage);
            info!("Downloaded static image data");
        }
        Err(e) => {
            warn!("Failed to download static image data: {e:?}");
        }
    }

    // Download conditional image data
    let conditional_image_url = format!(
        "http://{}/static/conditional_image_{}",
        config.server_address, config.device_name
    );
    match agent.get(&conditional_image_url).call() {
        Ok(response) => {
            let mut data = Vec::new();
            response.into_reader().read_to_end(&mut data)?;
            let prep_data: PrepareConditionalImageData = bincode::deserialize(&data)?;
            prepare_conditional_images(prep_data.images_data);
            info!("Downloaded conditional image data");
        }
        Err(e) => {
            warn!("Failed to download conditional image data: {e:?}");
        }
    }

    Ok(())
}

fn poll_data_loop(
    agent: &Agent,
    config: &HttpClientConfig,
    client_id: &str,
    ui_display_image_handle: &SharedImageHandle,
    render_busy_indicator: &Arc<Mutex<bool>>,
    sensor_value_history: &Arc<Mutex<Vec<Vec<SensorValue>>>>,
    fonts_data: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data_url = format!(
        "http://{}/data?client_id={}",
        config.server_address, client_id
    );

    loop {
        match agent.get(&data_url).timeout(POLL_TIMEOUT).call() {
            Ok(response) => {
                let mut data = Vec::new();
                response.into_reader().read_to_end(&mut data)?;
                handle_sensor_data(
                    &data,
                    ui_display_image_handle,
                    render_busy_indicator,
                    sensor_value_history,
                    fonts_data,
                );
            }
            Err(ureq::Error::Transport(e)) if e.kind() == ureq::ErrorKind::Io => {
                // Check if it's a timeout error
                if let Some(io_error) = e.source().and_then(|e| e.downcast_ref::<std::io::Error>())
                {
                    if io_error.kind() == std::io::ErrorKind::TimedOut {
                        // Timeout is expected - the server blocks until data is available
                        continue;
                    }
                }
                error!("Transport error polling data: {e:?}");
                return Err(e.into());
            }
            Err(e) => {
                error!("Error polling data: {e:?}");
                thread::sleep(RETRY_DELAY);
            }
        }
    }
}

fn handle_sensor_data(
    data: &[u8],
    ui_display_image_handle: &SharedImageHandle,
    render_busy_indicator: &Arc<Mutex<bool>>,
    sensor_value_history: &Arc<Mutex<Vec<Vec<SensorValue>>>>,
    fonts_data: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
) {
    info!("Received new sensor data: {} bytes", data.len());

    let transport_message: TransportMessage = match bincode::deserialize(data) {
        Ok(msg) => msg,
        Err(e) => {
            error!("Failed to deserialize transport message: {e:?}");
            return;
        }
    };

    let transport_type = transport_message.transport_type;
    let transport_data = transport_message.data;

    info!("Type: {transport_type:?}");

    match transport_type {
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
            let fonts_data = fonts_data.clone();

            thread::spawn(move || {
                // Begin rendering
                *render_busy_indicator.lock().unwrap() = true;

                // Define render closure
                let do_render = || -> Result<(), Box<dyn std::error::Error>> {
                    let render_data: RenderData = bincode::deserialize(&transport_data)?;

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
        _ => {
            warn!("Unexpected transport type in data stream: {transport_type:?}");
        }
    }
}

/// Prepare static data for rendering on the local filesystem.
fn prepare_static_data(assets: HashMap<String, Vec<u8>>, element_type: ElementType) {
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

/// Discover available sensor bridge servers on the network
pub fn discover_servers() -> Result<Vec<DiscoveredServer>, Box<dyn Error>> {
    info!("Starting server discovery...");

    // Use a random port to avoid conflicts and listen for broadcasts
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?; // Short timeout for non-blocking behavior
    socket.set_broadcast(true)?;

    let mut discovered_servers = Vec::new();
    let mut buf = [0; 1024];
    let start_time = Instant::now();

    info!("Listening for server broadcasts...");

    // Listen for server broadcasts with periodic checks
    while start_time.elapsed() < DISCOVERY_TIMEOUT {
        match socket.recv_from(&mut buf) {
            Ok((size, addr)) => {
                // Try to deserialize as ServiceDiscoveryMessage (bincode format from server)
                if let Ok(message) = bincode::deserialize::<ServiceDiscoveryMessage>(&buf[..size]) {
                    // Verify this is actually a sensor_bridge service
                    if message.service_name == "sensor_bridge" {
                        let server = DiscoveredServer {
                            ip: addr.ip().to_string(),
                            port: message.server_port,
                            service_name: message.service_name,
                            version: message.version,
                        };

                        // Avoid duplicates
                        if !discovered_servers
                            .iter()
                            .any(|s: &DiscoveredServer| s.ip == server.ip && s.port == server.port)
                        {
                            info!(
                                "Discovered server: {}:{} ({})",
                                server.ip, server.port, server.version
                            );
                            discovered_servers.push(server);
                        }
                    }
                }
            }
            Err(e) => {
                // Check if it's a timeout or other error
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock
                {
                    // Short sleep to prevent busy waiting
                    thread::sleep(Duration::from_millis(50));
                    continue;
                } else {
                    warn!("Error during discovery: {e:?}");
                    break;
                }
            }
        }
    }

    info!(
        "Discovery completed. Found {} servers",
        discovered_servers.len()
    );
    Ok(discovered_servers)
}

/// Try to detect the sensor bridge server address using automatic discovery
pub fn detect_server_address() -> Option<String> {
    info!("Attempting automatic server discovery...");

    match discover_servers() {
        Ok(servers) => {
            if !servers.is_empty() {
                let server = &servers[0];
                let address = server.address();
                info!("Using discovered server: {address}");
                Some(address)
            } else {
                warn!("No servers discovered, falling back to localhost");
                Some(format!("127.0.0.1:{DEFAULT_SERVER_PORT}"))
            }
        }
        Err(e) => {
            warn!("Server discovery failed: {e:?}, falling back to localhost");
            Some(format!("127.0.0.1:{DEFAULT_SERVER_PORT}"))
        }
    }
}

/// Create display configuration based on current screen resolution
pub fn create_display_config_from_screen() -> DisplayConfig {
    // This could be enhanced to detect actual screen properties
    DisplayConfig {
        resolution_width: 128,
        resolution_height: 64,
        elements: Vec::new(),
    }
}
