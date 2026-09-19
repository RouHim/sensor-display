use std::collections::VecDeque;
use std::error;
use std::fmt;
use std::io::Read;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use local_ip_address::local_ip;
use log::{error, info, warn};
use mac_address::get_mac_address;
use sensor_core::{RenderData, SensorValue, StaticClientData};
use serde::{Deserialize, Serialize};

use crate::renderer;
use crate::static_data;
use crate::ui::{SharedImageHandle, SharedStatus};

/// Fallback HTTP port when neither the configuration nor the command line provides one.
pub const DEFAULT_SERVER_PORT: u16 = 55555;
/// Polling cadence: one poll per second, scheduled on a fixed grid.
pub const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Re-check cadence while the protocol version does not match (never faster).
pub const PROTOCOL_RECHECK_INTERVAL: Duration = Duration::from_secs(60);
/// Retry cadence of the bootstrap phase (unchanged behavior).
pub const BOOTSTRAP_RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// Whether a remote protocol version is compatible with this build.
/// A missing version (old bridge) counts as incompatible.
pub fn protocol_compatible(remote_version: Option<u32>) -> bool {
    remote_version == Some(sensor_core::PROTOCOL_VERSION)
}

/// Errors the polling loop distinguishes.
#[derive(Debug)]
pub enum ClientError {
    /// Bridge protocol version differs from (or is missing compared to) ours.
    ProtocolMismatch(Option<u32>),
    NotRegistered,
    NotActive,
    /// Bridge has not completed its first sampling pass yet.
    NoSensorSample,
    Other(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::ProtocolMismatch(remote) => match remote {
                Some(version) => write!(
                    formatter,
                    "protocol version mismatch (bridge: {}, display: {})",
                    version,
                    sensor_core::PROTOCOL_VERSION
                ),
                None => write!(
                    formatter,
                    "bridge does not report a protocol version (display: {})",
                    sensor_core::PROTOCOL_VERSION
                ),
            },
            ClientError::NotRegistered => write!(formatter, "client not registered"),
            ClientError::NotActive => write!(formatter, "client not active"),
            ClientError::NoSensorSample => write!(formatter, "bridge has no sensor sample yet"),
            ClientError::Other(message) => write!(formatter, "{message}"),
        }
    }
}

impl error::Error for ClientError {}

/// Static data as delivered by the bridge, together with its revision.
#[derive(Debug)]
pub struct StaticDataEnvelope {
    pub revision: String,
    pub data: static_data::StaticDataResult,
}

/// Response body of `POST /api/static-data/ack`.
#[derive(Debug, Deserialize)]
struct StaticDataAckResponse {
    /// True only when the acked revision was still the client's current one.
    pending_cleared: bool,
}

/// Client lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientState {
    /// Register and load the initial static data.
    Bootstrap,
    /// Normal 1 Hz polling.
    Active,
    /// Protocol mismatch: no payload decoding, 60 s re-checks.
    UpdateRequired,
}

/// Maps transport errors onto the states the loop reacts to.
fn map_transport_error(err: ureq::Error) -> ClientError {
    match err {
        ureq::Error::StatusCode(404) => ClientError::NotRegistered,
        ureq::Error::StatusCode(403) => ClientError::NotActive,
        ureq::Error::StatusCode(503) => ClientError::NoSensorSample,
        other => ClientError::Other(other.to_string()),
    }
}

/// Reads a header value as UTF-8 string.
fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

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

        // Canonical client identifier (the same normalization the bridge applies)
        let normalized_mac = sensor_core::normalize_mac(&mac_address);

        let ip_address = local_ip()
            .map_err(|e| format!("Failed to get local IP: {e}"))?
            .to_string();

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();

        Ok(Self {
            agent,
            server_url,
            mac_address: normalized_mac,
            ip_address,
            resolution_width: resolution.0,
            resolution_height: resolution.1,
        })
    }

    /// Registers with the bridge and verifies the protocol version.
    pub fn register(&self) -> Result<(), ClientError> {
        let registration_data = ClientRegistrationRequestData {
            mac_address: self.mac_address.clone(),
            ip_address: self.ip_address.clone(),
            resolution_width: self.resolution_width,
            resolution_height: self.resolution_height,
            name: None,
        };

        info!("Registering client with MAC: {}", self.mac_address);

        let mut response = self
            .agent
            .post(&format!("{}/api/register", self.server_url))
            .send_json(&registration_data)
            .map_err(map_transport_error)?;

        let result: serde_json::Value = response.body_mut().read_json().map_err(|err| {
            ClientError::Other(format!("Failed to parse registration response: {err}"))
        })?;

        if result["success"] != true {
            return Err(ClientError::Other("Registration failed".to_string()));
        }

        let remote_version = result
            .get("protocol_version")
            .and_then(|value| value.as_u64())
            .map(|value| value as u32);
        if !protocol_compatible(remote_version) {
            return Err(ClientError::ProtocolMismatch(remote_version));
        }

        info!("Registration successful");
        Ok(())
    }

    /// Reads the protocol version from the bridge health endpoint.
    pub fn get_health(&self) -> Result<Option<u32>, ClientError> {
        let mut response = self
            .agent
            .get(&format!("{}/health", self.server_url))
            .call()
            .map_err(map_transport_error)?;

        let body: serde_json::Value = response
            .body_mut()
            .read_json()
            .map_err(|err| ClientError::Other(format!("Failed to parse health response: {err}")))?;

        Ok(body
            .get("protocol_version")
            .and_then(|value| value.as_u64())
            .map(|value| value as u32))
    }

    /// Fetches static data. Verifies the protocol version BEFORE decoding, and
    /// returns the revision identifier for the confirmation call.
    pub fn get_static_data(&self) -> Result<StaticDataEnvelope, ClientError> {
        let url = format!(
            "{}/api/static-data?mac_address={}",
            self.server_url, self.mac_address
        );

        let mut response = self.agent.get(&url).call().map_err(map_transport_error)?;

        let remote_version =
            header(&response, "x-protocol-version").and_then(|value| value.parse::<u32>().ok());
        if !protocol_compatible(remote_version) {
            return Err(ClientError::ProtocolMismatch(remote_version));
        }

        let revision = header(&response, "x-static-data-revision").ok_or_else(|| {
            ClientError::Other("static data response without revision header".to_string())
        })?;

        let mut binary_data = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut binary_data)
            .map_err(|err| ClientError::Other(format!("Failed to read static data: {err}")))?;

        info!("Static data received, {} bytes", binary_data.len());

        // Deserialize the single StaticClientData struct from binary data
        let static_client_data: StaticClientData =
            crate::serialization::decode(&binary_data).map_err(ClientError::Other)?;

        info!("  - {} font families", static_client_data.text_data.len());
        info!(
            "  - {} static images",
            static_client_data.static_image_data.len()
        );
        info!(
            "  - {} conditional image elements",
            static_client_data.conditional_image_data.len()
        );

        Ok(StaticDataEnvelope {
            revision,
            data: static_data::StaticDataResult {
                text_data: static_client_data.text_data,
                static_image_data: static_client_data.static_image_data,
                conditional_image_data: static_client_data.conditional_image_data,
            },
        })
    }

    /// Confirms that a delivered static-data revision was persisted.
    ///
    /// Returns whether the bridge actually cleared its pending reload flag. A
    /// confirmation for a revision that is no longer current is answered
    /// successfully, but with `pending_cleared: false`.
    pub fn ack_static_data(&self, revision: &str) -> Result<bool, ClientError> {
        self.agent
            .post(&format!("{}/api/static-data/ack", self.server_url))
            .send_json(serde_json::json!({
                "mac_address": self.mac_address,
                "revision": revision
            }))
            .and_then(|mut response| {
                response
                    .body_mut()
                    .read_json::<StaticDataAckResponse>()
                    .map(|ack| ack.pending_cleared)
            })
            .map_err(map_transport_error)
    }

    /// Get sensor data from the server
    pub fn get_sensor_data(&self) -> Result<SensorDataResponse, ClientError> {
        let url = format!(
            "{}/api/sensor-data?mac_address={}",
            self.server_url, self.mac_address
        );

        let mut response = self.agent.get(&url).call().map_err(map_transport_error)?;

        response
            .body_mut()
            .read_json::<SensorDataResponse>()
            .map_err(|err| {
                ClientError::Other(format!("Failed to parse sensor data response: {err}"))
            })
    }
}

/// Fixed 1 Hz grid the polling loop walks along.
///
/// The deadline only ever advances by exactly `POLL_INTERVAL`, so request latency
/// and re-registrations can never accumulate drift. Re-basing it on the time a
/// poll finished (`Instant::now()` inside the loop) would defeat that; this unit
/// exists so the scheduling decision is exercised by tests instead of duplicated
/// in them.
struct PollSchedule {
    deadline: Instant,
}

impl PollSchedule {
    /// Anchors the grid at `now`.
    fn new(now: Instant) -> Self {
        Self { deadline: now }
    }

    /// Advances to the next grid slot and returns how long to wait for it, given
    /// that the previous poll finished at `now`. Saturates to zero when the slot
    /// has already passed.
    fn wait_before_next_poll(&mut self, now: Instant) -> Duration {
        self.deadline += POLL_INTERVAL;
        self.deadline.saturating_duration_since(now)
    }
}

/// Starts the HTTP client thread.
pub fn start_http_client(
    ui_display_image_handle: SharedImageHandle,
    client_status: SharedStatus,
    font_cache: Arc<RwLock<lru::LruCache<String, ab_glyph::FontVec>>>,
    server_host: String,
    server_port: Option<u16>,
    resolution: (u16, u16),
) {
    let render_busy_indicator = Arc::new(RwLock::new(false));
    let sensor_value_history: Arc<RwLock<VecDeque<Vec<SensorValue>>>> =
        Arc::new(RwLock::new(VecDeque::new()));

    std::thread::spawn(move || {
        let client = match SensorBridgeClient::new(&server_host, server_port, resolution) {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to create HTTP client: {e}");
                return;
            }
        };

        let mut state = ClientState::Bootstrap;
        let mut poll_schedule: Option<PollSchedule> = None;
        let mut last_protocol_check: Option<Instant> = None;

        loop {
            match state {
                ClientState::Bootstrap => match bootstrap(&client) {
                    Ok(()) => {
                        state = ClientState::Active;
                        poll_schedule = Some(PollSchedule::new(Instant::now()));
                        last_protocol_check = Some(Instant::now());
                        info!("Starting sensor data polling loop");
                        info!("Note: Client must be activated in the server UI to receive data");
                    }
                    Err(ClientError::ProtocolMismatch(remote)) => {
                        enter_update_required(&client_status, remote);
                        state = ClientState::UpdateRequired;
                    }
                    Err(e) => {
                        error!("{e}. Retrying in 5 seconds...");
                        std::thread::sleep(BOOTSTRAP_RETRY_INTERVAL);
                    }
                },
                ClientState::Active => {
                    // The schedule is kept across polls (and across re-registrations,
                    // which happen inside `poll_once`), so only one interval is ever
                    // waited per poll, no matter how long the poll itself took.
                    let schedule =
                        poll_schedule.get_or_insert_with(|| PollSchedule::new(Instant::now()));

                    let next_state = poll_once(
                        &client,
                        &client_status,
                        &ui_display_image_handle,
                        &render_busy_indicator,
                        &sensor_value_history,
                        &font_cache,
                        &mut last_protocol_check,
                    );

                    if let Some(next_state) = next_state {
                        state = next_state;
                        poll_schedule = None;
                    } else {
                        let sleep = schedule.wait_before_next_poll(Instant::now());
                        std::thread::sleep(sleep);
                    }
                }
                ClientState::UpdateRequired => {
                    std::thread::sleep(PROTOCOL_RECHECK_INTERVAL);
                    match client.get_health() {
                        Ok(remote) if protocol_compatible(remote) => {
                            info!("Bridge protocol matches again, resuming");
                            *client_status.write().unwrap() = None;
                            state = ClientState::Bootstrap;
                        }
                        Ok(remote) => info!(
                            "Still waiting for a matching protocol version (bridge: {:?})",
                            remote
                        ),
                        Err(e) => warn!("Health check failed: {e}"),
                    }
                }
            }
        }
    });
}

/// Registers and loads the initial static data.
fn bootstrap(client: &SensorBridgeClient) -> Result<(), ClientError> {
    client.register()?;
    info!("Registration successful, now getting initial static data");

    let envelope = client.get_static_data()?;
    static_data::persist_static_data_to_disk(&envelope.data).map_err(|err| {
        ClientError::Other(format!("Failed to persist initial static data: {err}"))
    })?;

    if let Err(e) = client.ack_static_data(&envelope.revision) {
        warn!("Failed to confirm initial static data: {e}");
    }

    info!("Successfully registered with server and loaded initial static data");
    Ok(())
}

/// Persists a reloaded payload and reports the bridge's confirmation truthfully.
///
/// The bridge answers a confirmation for a revision that is no longer current
/// with `pending_cleared: false`, which is a successful request but not a
/// confirmation, so it is logged as a warning naming the unmatched revision.
fn confirm_reloaded_static_data(client: &SensorBridgeClient, envelope: &StaticDataEnvelope) {
    if let Err(e) = static_data::persist_static_data_to_disk(&envelope.data) {
        error!("Failed to persist updated static data: {}", e);
        return;
    }

    match client.ack_static_data(&envelope.revision) {
        Ok(true) => info!("Static data reloaded and confirmed"),
        Ok(false) => warn!(
            "Static data reloaded, but the bridge did not clear the pending flag for revision {}",
            envelope.revision
        ),
        Err(e) => warn!("Failed to confirm static data: {e}"),
    }
}

/// One poll cycle. Returns `Some(next_state)` when the loop must switch state.
fn poll_once(
    client: &SensorBridgeClient,
    client_status: &SharedStatus,
    ui_display_image_handle: &SharedImageHandle,
    render_busy_indicator: &Arc<RwLock<bool>>,
    sensor_value_history: &Arc<RwLock<VecDeque<Vec<SensorValue>>>>,
    font_cache: &Arc<RwLock<lru::LruCache<String, ab_glyph::FontVec>>>,
    last_protocol_check: &mut Option<Instant>,
) -> Option<ClientState> {
    match client.get_sensor_data() {
        Ok(response) => {
            info!(
                "Received sensor data with {} sensor values",
                response.render_data.sensor_values.len()
            );

            if response.static_data_reload_required {
                info!("Static data reload required, fetching updated static data");
                match client.get_static_data() {
                    Ok(envelope) => confirm_reloaded_static_data(client, &envelope),
                    Err(ClientError::ProtocolMismatch(remote)) => {
                        enter_update_required(client_status, remote);
                        return Some(ClientState::UpdateRequired);
                    }
                    Err(e) => error!("Failed to reload static data: {}", e),
                }
            }

            handle_render_data(
                ui_display_image_handle,
                render_busy_indicator,
                sensor_value_history,
                font_cache,
                response.render_data,
                client.resolution_width,
                client.resolution_height,
            );
        }
        Err(ClientError::ProtocolMismatch(remote)) => {
            enter_update_required(client_status, remote);
            return Some(ClientState::UpdateRequired);
        }
        Err(ClientError::NotActive) => {
            warn!("Client is not active. Please activate in the server UI.");
        }
        Err(ClientError::NotRegistered) => {
            warn!("Client not registered. Re-registering...");
            match client.register() {
                Ok(()) => info!("Re-registration successful"),
                Err(ClientError::ProtocolMismatch(remote)) => {
                    enter_update_required(client_status, remote);
                    return Some(ClientState::UpdateRequired);
                }
                Err(e) => error!("Re-registration failed: {e}"),
            }
        }
        Err(ClientError::NoSensorSample) => {
            warn!("Bridge has no sensor sample yet");
        }
        Err(e) => error!("Error polling sensor data: {e}"),
    }

    // Periodic protocol check while running (at most once per minute).
    let check_due = last_protocol_check
        .map(|last| last.elapsed() >= PROTOCOL_RECHECK_INTERVAL)
        .unwrap_or(true);
    if check_due {
        *last_protocol_check = Some(Instant::now());
        match client.get_health() {
            Ok(remote) if !protocol_compatible(remote) => {
                enter_update_required(client_status, remote);
                return Some(ClientState::UpdateRequired);
            }
            Ok(_) => {}
            Err(e) => warn!("Health check failed: {e}"),
        }
    }

    None
}

/// Enters the update-required state: visible to the user, no payload decoding.
fn enter_update_required(client_status: &SharedStatus, remote: Option<u32>) {
    let message = format!(
        "Update required\n\nBridge protocol:\t{}\nDisplay protocol:\t{}",
        remote
            .map(|version| version.to_string())
            .unwrap_or_else(|| "unknown (old bridge)".to_string()),
        sensor_core::PROTOCOL_VERSION
    );
    error!("{message}");
    *client_status.write().unwrap() = Some(message);
}

/// Handle render data received from the server - now uses filesystem cache
fn handle_render_data(
    ui_display_image_handle: &SharedImageHandle,
    render_busy_indicator: &Arc<RwLock<bool>>,
    sensor_value_history: &Arc<RwLock<VecDeque<Vec<SensorValue>>>>,
    font_cache: &Arc<RwLock<lru::LruCache<String, ab_glyph::FontVec>>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_remote_protocol_version_is_incompatible() {
        assert!(
            !protocol_compatible(None),
            "an old bridge that does not send a protocol version must be update-required"
        );
        assert!(protocol_compatible(Some(sensor_core::PROTOCOL_VERSION)));
        assert!(!protocol_compatible(Some(
            sensor_core::PROTOCOL_VERSION + 1
        )));
    }

    #[test]
    fn slow_requests_do_not_shift_the_poll_schedule() {
        // Drives the production scheduling unit: a 300 ms request per cycle must
        // still wait 700 ms for every following poll, and the grid must stay
        // anchored at `start` (re-basing on completion time would fail here).
        let start = Instant::now();
        let mut schedule = PollSchedule::new(start);
        let mut simulated_now = start;

        for _ in 0..10 {
            simulated_now += Duration::from_millis(300); // request latency
            let sleep = schedule.wait_before_next_poll(simulated_now);
            assert_eq!(sleep, Duration::from_millis(700));
            simulated_now += sleep;
        }

        // Ten polls on a 1 s grid: the schedule never re-anchored, so the
        // simulated clock sits exactly on the tenth grid slot.
        assert_eq!(simulated_now - start, POLL_INTERVAL * 10);
    }

    #[test]
    fn a_deadline_in_the_past_does_not_sleep() {
        let start = Instant::now();
        sensor_core::sleep_until(start - Duration::from_secs(1));
        assert!(start.elapsed() < Duration::from_millis(50));
    }
}
