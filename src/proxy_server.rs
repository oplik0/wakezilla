use crate::connection_pool::ConnectionPool;
use anyhow::Result;
use axum::{
    body::Body,
    extract::{Json as JsonExtract, Path, Query, State},
    http::{header, Method, Request, Response, StatusCode},
    response::{IntoResponse, Json, Redirect},
    routing::{delete, get, post, put},
    Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::{net::TcpListener, sync::RwLock};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::{debug, error, info};
use validator::Validate;

use crate::forward;
use crate::scanner;
use crate::web::{self, AppState, DeleteForm, Machine};
use crate::wol;
use include_dir::{include_dir, Dir};
use mime_guess::from_path;
use std::path::{Component, Path as StdPath};

static FRONTEND_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/frontend/dist");

fn respond_with_file(file: &include_dir::File<'_>) -> Response<Body> {
    let mime = from_path(file.path()).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(file.contents().to_vec()))
        .unwrap()
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

fn asset_response(path: &str) -> Response<Body> {
    let trimmed = path.trim_start_matches('/');
    let target = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };

    if StdPath::new(target)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return not_found();
    }

    if let Some(file) = FRONTEND_DIST.get_file(target) {
        return respond_with_file(file);
    }

    if !target.contains('.') {
        if let Some(index) = FRONTEND_DIST.get_file("index.html") {
            return respond_with_file(index);
        }
    }

    not_found()
}

async fn serve_index() -> Response<Body> {
    // if debug build, redirect to vite dev server (localhost:3000)
    if cfg!(debug_assertions) {
        return Redirect::to("http://localhost:8080").into_response();
    }
    asset_response("")
}

async fn spa_fallback(req: Request<Body>) -> Response<Body> {
    match req.method() {
        &Method::GET | &Method::HEAD => {
            let mut response = asset_response(req.uri().path());
            if req.method() == Method::HEAD {
                *response.body_mut() = Body::empty();
            }
            response
        }
        _ => not_found(),
    }
}

pub async fn start(port: u16) -> Result<()> {
    let initial_machines = web::load_machines().unwrap_or_default();

    // Create connection pool and start cleanup task
    let connection_pool = ConnectionPool::new();
    let cleanup_handle = connection_pool.start_cleanup_task();

    // Spawn cleanup task
    tokio::spawn(async move {
        cleanup_handle.await.ok();
    });

    let state = AppState {
        machines: Arc::new(RwLock::new(initial_machines.clone())),
        proxies: Arc::new(RwLock::new(HashMap::new())),
        connection_pool,
    };

    for machine in &initial_machines {
        web::start_proxy_if_configured(machine, &state);
    }

    let app = build_router(state.clone());
    let endpoints = api_routes(state.clone());
    let app = app.merge(endpoints);

    let cors_layer = CorsLayer::permissive();

    let app = app.layer(ServiceBuilder::new().layer(cors_layer).into_inner());
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    info!("listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}

pub fn api_routes(state: AppState) -> Router {
    Router::new()
        .route("/api/interfaces", get(list_interfaces_handler))
        .route("/api/scan", get(scan_network_handler))
        .route(
            "/api/machines",
            get(show_machines_api).post(add_machine_api),
        )
        .route("/api/machines/:mac", get(get_machine_details_api))
        .route("/api/machines/:mac", put(update_machine_api))
        .route(
            "/api/machines/:mac/remote-turn-off",
            post(api_turn_off_remote_machine),
        )
        .route("/api/machines/:mac/wake", post(api_wake_machine))
        .route("/api/machines/delete", delete(delete_machine_api))
        .with_state(state)
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .fallback(spa_fallback)
        .with_state(state)
}

async fn scan_network_handler(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    let interface = params.get("interface").map(|s| s.as_str());
    match scanner::scan_network_with_interface(interface).await {
        Ok(devices) => Ok(Json(devices)),
        Err(e) => {
            error!("Network scan failed: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn list_interfaces_handler() -> impl IntoResponse {
    match scanner::list_interfaces().await {
        Ok(interfaces) => Ok(Json(interfaces)),
        Err(e) => {
            error!("Failed to list interfaces: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn add_machine_api(
    State(state): State<AppState>,
    JsonExtract(payload): JsonExtract<web::AddMachineForm>,
) -> impl IntoResponse {
    if let Err(errors) = payload.validate() {
        let errors_map = errors
            .field_errors()
            .iter()
            .map(|(key, value)| {
                let error_messages: Vec<String> =
                    value.iter().map(|error| error.code.to_string()).collect();
                (key.to_string(), error_messages)
            })
            .collect::<HashMap<_, _>>();

        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "errors": errors_map })),
        );
    }
    let new_machine = Machine {
        mac: payload.mac,
        ip: payload.ip.parse().expect("Invalid IP address"),
        name: payload.name,
        description: payload.description,
        turn_off_port: payload.turn_off_port,
        can_be_turned_off: payload.can_be_turned_off,
        request_rate: web::RequestRateConfig {
            max_requests: payload.requests_per_hour.unwrap_or(60),
            period_minutes: payload.period_minutes.unwrap_or(60),
        },
        port_forwards: payload.port_forwards.unwrap_or_default(),
    };
    let mut machines = state.machines.write().await;
    web::start_proxy_if_configured(&new_machine, &state);
    machines.push(new_machine);

    if let Err(e) = web::save_machines(&machines) {
        error!("Error saving machines: {}", e);
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to save machines" })),
        );
    }
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({ "status": "Machine added" })),
    )
}

async fn show_machines_api(State(state): State<AppState>) -> impl IntoResponse {
    let mut machines = state.machines.read().await.clone();
    machines.reverse();
    Json(machines)
}

async fn get_machine_details_api(
    State(state): State<AppState>,
    Path(mac): Path<String>,
) -> Result<Json<Machine>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let machines = state.machines.read().await;
    if let Some(machine) = machines.iter().find(|m| m.mac == mac).cloned() {
        Ok(Json(machine))
    } else {
        Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Machine not found" })),
        ))
    }
}

async fn update_machine_api(
    State(state): State<AppState>,
    Path(mac): Path<String>,
    JsonExtract(payload): JsonExtract<web::MachinePayload>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let mut machines = state.machines.write().await;

    // check if the machine exists
    let exists = machines.iter().any(|m| m.mac == mac);
    if !exists {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Machine not found" })),
        ));
    }
    // remove the machine to update
    machines.retain(|m| m.mac != mac);

    let new_machine = Machine {
        mac: payload.mac.clone(),
        ip: payload.ip.parse().expect("Invalid IP address"),
        name: payload.name.clone(),
        description: payload.description.clone(),
        turn_off_port: payload.turn_off_port,
        can_be_turned_off: payload.can_be_turned_off,
        request_rate: web::RequestRateConfig {
            max_requests: payload.requests_per_hour.unwrap_or(1000),
            period_minutes: payload.period_minutes.unwrap_or(60),
        },
        port_forwards: payload.port_forwards.clone().unwrap_or_default(),
    };

    if let Err(e) = web::save_machines(&machines) {
        error!("Error saving machines: {}", e);
        return Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to save machines" })),
        ));
    }

    // Restart proxy if needed
    web::start_proxy_if_configured(&new_machine, &state);
    machines.push(new_machine);

    Ok(Json(serde_json::json!({ "status": "Machine updated" })))
}

async fn delete_machine_api(
    State(state): State<AppState>,
    JsonExtract(payload): JsonExtract<DeleteForm>,
) -> impl IntoResponse {
    // Stop all proxies associated with this machine
    info!("Deleting machine with MAC: {}", payload.mac);
    let mut proxies = state.proxies.write().await;
    proxies.retain(|key, tx| {
        if key.starts_with(&payload.mac) {
            if tx.send(false).is_ok() {
                info!("Sent stop signal to proxy for MAC/key: {}", key);
            }
            false // Remove the entry
        } else {
            true // Keep the entry
        }
    });
    drop(proxies); // Release the write lock

    let mut machines = state.machines.write().await;

    // Remove connections from pool for this machine's IP
    if let Some(machine) = machines.iter().find(|m| m.mac == payload.mac) {
        let target_addr = SocketAddr::from((machine.ip, 0));
        state.connection_pool.remove_target(target_addr).await;
        debug!("Removed connections from pool for machine {}", machine.ip);
    }

    machines.retain(|m| m.mac != payload.mac);

    if let Err(e) = web::save_machines(&machines) {
        error!("Error saving machines: {}", e);
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to save machines" })),
        );
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({ "status": "Machine deleted" })),
    )
}

async fn execute_remote_turn_off(state: &AppState, mac: &str) -> (axum::http::StatusCode, String) {
    let machine = {
        let machines = state.machines.read().await;
        machines.iter().find(|m| m.mac == mac).cloned()
    };

    if let Some(machine) = machine {
        if let Some(port) = machine.turn_off_port {
            info!("Sending turn-off request to {}:{}", machine.ip, port);
            match forward::turn_off_remote_machine(&machine.ip.to_string(), port).await {
                Ok(_) => {
                    return (
                        axum::http::StatusCode::OK,
                        format!("Sent turn-off request to {}", mac),
                    );
                }
                Err(e) => {
                    return (
                        axum::http::StatusCode::BAD_GATEWAY,
                        format!("Failed to send turn-off request: {}", e),
                    );
                }
            }
        } else {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!("No turn-off port configured for {}", mac),
            );
        }
    }

    (
        axum::http::StatusCode::NOT_FOUND,
        format!("Machine {} not found", mac),
    )
}

async fn api_turn_off_remote_machine(
    State(state): State<AppState>,
    Path(mac): Path<String>,
) -> impl IntoResponse {
    let (status, message) = execute_remote_turn_off(&state, &mac).await;
    (
        status,
        Json(serde_json::json!({
            "message": message,
        })),
    )
}

async fn execute_wake(mac_input: &str) -> (axum::http::StatusCode, String) {
    let parsed_mac = match wol::parse_mac(mac_input) {
        Ok(mac) => mac,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Invalid MAC address '{}': {}", mac_input, e),
            );
        }
    };

    let bcast = std::net::Ipv4Addr::new(255, 255, 255, 255);
    let port = 9; // Default WOL port
    let count = 3;

    match crate::wol::send_packets(&parsed_mac, bcast, port, count, &Default::default()).await {
        Ok(_) => (
            axum::http::StatusCode::OK,
            format!("Sent WOL packet to {}", mac_input),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send WOL packet to {}: {}", mac_input, e),
        ),
    }
}

async fn api_wake_machine(Path(mac): Path<String>) -> impl IntoResponse {
    let (status, message) = execute_wake(&mac).await;
    (
        status,
        Json(serde_json::json!({
            "message": message,
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ENV_LOCK;
    use axum::body::to_bytes;
    use axum::extract::Form;
    use axum::extract::Path;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::Json;
    use std::collections::HashMap;
    use std::io::ErrorKind;
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::{watch, Mutex as AsyncMutex, RwLock};

    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set_path(key: &'static str, value: &std::path::Path) -> Self {
            let original = std::env::var(key).ok();
            std::env::set_var(key, value.as_os_str());
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(ref original) = self.original {
                std::env::set_var(self.key, original);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn state_with_machines(machines: Vec<Machine>) -> AppState {
        AppState {
            machines: Arc::new(RwLock::new(machines)),
            proxies: Arc::new(RwLock::new(HashMap::new())),
            connection_pool: ConnectionPool::new(),
        }
    }

    fn sample_machine() -> Machine {
        Machine {
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            ip: Ipv4Addr::new(10, 0, 0, 1),
            name: "Sample".to_string(),
            description: Some("Desc".to_string()),
            turn_off_port: Some(8080),
            can_be_turned_off: false,
            request_rate: web::RequestRateConfig {
                max_requests: 5,
                period_minutes: 30,
            },
            port_forwards: vec![],
        }
    }

    #[tokio::test]
    async fn machine_detail_returns_not_found_when_missing() {
        let state = state_with_machines(vec![]);
        let response = machine_detail(State(state), Path("AA:BB:CC:DD:EE:FF".to_string()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn turn_off_remote_machine_handles_missing_machine() {
        let state = state_with_machines(vec![]);
        let (status, message) = turn_off_remote_machine(
            State(state),
            Form(RemoteTurnOffForm {
                mac: "AA:BB:CC:DD:EE:FF".to_string(),
            }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(message.contains("not found"));
    }

    #[tokio::test]
    async fn turn_off_remote_machine_requires_port() {
        let mut machine = sample_machine();
        machine.turn_off_port = None;
        let state = state_with_machines(vec![machine]);
        let (status, message) = turn_off_remote_machine(
            State(state),
            Form(RemoteTurnOffForm {
                mac: "AA:BB:CC:DD:EE:FF".to_string(),
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(message.contains("No turn-off port"));
    }

    #[tokio::test]
    async fn api_turn_off_remote_machine_returns_json_message() {
        let state = state_with_machines(vec![]);
        let response =
            api_turn_off_remote_machine(State(state), Path("AA:BB:CC:DD:EE:FF".to_string()))
                .await
                .into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body to be readable");
        let json: serde_json::Value =
            serde_json::from_slice(&body_bytes).expect("response to be valid json");
        assert!(
            json["message"]
                .as_str()
                .unwrap_or_default()
                .contains("not found"),
            "expected message to mention missing machine"
        );
    }

    #[tokio::test]
    async fn add_machine_persists_new_entry() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp_dir = tempdir().expect("failed to create temp dir");
        let file_path = tmp_dir.path().join("machines.json");
        let _guard = EnvGuard::set_path("WAKEZILLA_MACHINES_DB_PATH", &file_path);

        let state = state_with_machines(vec![]);
        let form = web::AddMachineForm {
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            ip: "192.168.1.10".to_string(),
            name: "New machine".to_string(),
            description: Some("Test machine".to_string()),
            turn_off_port: Some(8080),
            can_be_turned_off: true,
            requests_per_hour: Some(12),
            period_minutes: Some(6),
            port_forwards: None,
        };

        let response = add_machine(State(state.clone()), Form(form)).await;
        assert_eq!(response.into_response().status(), StatusCode::SEE_OTHER);

        let machines = state.machines.read().await;
        assert_eq!(machines.len(), 1);
        assert_eq!(machines[0].mac, "AA:BB:CC:DD:EE:FF");
        assert_eq!(machines[0].name, "New machine");
        assert_eq!(machines[0].request_rate.max_requests, 12);
        assert_eq!(machines[0].request_rate.period_minutes, 6);
    }

    #[tokio::test]
    async fn add_machine_returns_errors_for_invalid_payload() {
        let state = state_with_machines(vec![]);
        let form = web::AddMachineForm {
            mac: "invalid".to_string(),
            ip: "not-an-ip".to_string(),
            name: "Bad".to_string(),
            description: None,
            turn_off_port: None,
            can_be_turned_off: false,
            requests_per_hour: None,
            period_minutes: None,
            port_forwards: None,
        };

        let response = add_machine(State(state.clone()), Form(form)).await;
        assert_eq!(response.into_response().status(), StatusCode::OK);
        assert!(state.machines.read().await.is_empty());
    }

    #[tokio::test]
    async fn turn_off_remote_machine_sends_request_when_configured() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                eprintln!(
                    "skipping turn_off_remote_machine_sends_request_when_configured: {}",
                    err
                );
                return;
            }
            Err(err) => panic!("failed to bind listener: {err}"),
        };
        let addr = listener.local_addr().expect("failed to get addr");
        let received = Arc::new(AsyncMutex::new(None));
        let received_clone = received.clone();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 1024];
                if let Ok(n) = socket.read(&mut buf).await {
                    if n > 0 {
                        let request = String::from_utf8_lossy(&buf[..n]).to_string();
                        *received_clone.lock().await = Some(request);
                    }
                }
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await;
            }
        });

        let mut machine = sample_machine();
        machine.turn_off_port = Some(addr.port());
        machine.ip = addr.ip().to_string().parse().unwrap();
        let state = state_with_machines(vec![machine]);

        let (status, message) = turn_off_remote_machine(
            State(state),
            Form(RemoteTurnOffForm {
                mac: "AA:BB:CC:DD:EE:FF".to_string(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(message.contains("Sent turn-off request"));
        let request = received
            .lock()
            .await
            .clone()
            .expect("expected request to be captured");
        assert!(request.starts_with("POST /machines/turn-off"));
    }

    #[tokio::test]
    async fn wake_machine_rejects_invalid_mac() {
        let (status, message) = wake_machine(Form(WakeForm {
            mac: "invalid".to_string(),
        }))
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(message.contains("Invalid MAC address"));
    }

    #[tokio::test]
    async fn api_wake_machine_returns_json_for_invalid_mac() {
        let response = api_wake_machine(Path("invalid".to_string()))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body to be readable");
        let json: serde_json::Value =
            serde_json::from_slice(&body_bytes).expect("response to be valid json");
        assert!(json["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Invalid MAC"));
    }

    #[tokio::test]
    async fn machine_health_reports_unknown_without_port() {
        let Json(value) = machine_health(Json(MachineHealthRequest {
            ip: "127.0.0.1".to_string(),
            turn_off_port: None,
        }))
        .await;
        assert_eq!(value["status"], "unknown");
    }

    #[tokio::test]
    async fn machine_health_reports_offline_with_error_status() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                eprintln!(
                    "skipping machine_health_reports_offline_with_error_status: {}",
                    err
                );
                return;
            }
            Err(err) => panic!("failed to bind listener: {err}"),
        };
        let addr = listener.local_addr().expect("failed to read listener addr");

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let _ = socket
                    .write_all(b"HTTP/1.1 500 INTERNAL SERVER ERROR\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        });

        let Json(value) = machine_health(Json(MachineHealthRequest {
            ip: addr.ip().to_string(),
            turn_off_port: Some(addr.port()),
        }))
        .await;

        assert_eq!(value["status"], "offline");
        assert_eq!(value["http_status"], 500);
    }

    #[tokio::test]
    async fn machine_health_reports_online_with_successful_response() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                eprintln!(
                    "skipping machine_health_reports_online_with_successful_response: {}",
                    err
                );
                return;
            }
            Err(err) => panic!("failed to bind listener: {err}"),
        };
        let addr = listener.local_addr().expect("failed to read listener addr");

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await;
            }
        });

        let Json(value) = machine_health(Json(MachineHealthRequest {
            ip: addr.ip().to_string(),
            turn_off_port: Some(addr.port()),
        }))
        .await;

        assert_eq!(value["status"], "online");
        assert_eq!(value["http_status"], 200);
    }

    #[tokio::test]
    async fn update_machine_config_applies_changes() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp_dir = tempdir().expect("failed to create temp dir");
        let file_path = tmp_dir.path().join("machines.json");
        let _guard = EnvGuard::set_path("WAKEZILLA_MACHINES_DB_PATH", &file_path);

        let state = state_with_machines(vec![sample_machine()]);
        let mut payload = HashMap::new();
        payload.insert("mac".to_string(), "AA:BB:CC:DD:EE:FF".to_string());
        payload.insert("name".to_string(), "Updated".to_string());
        payload.insert("description".to_string(), "New description".to_string());
        payload.insert("can_be_turned_off".to_string(), "true".to_string());
        payload.insert("requests_per_hour".to_string(), "15".to_string());
        payload.insert("period_minutes".to_string(), "5".to_string());
        payload.insert("turn_off_port".to_string(), "8080".to_string());

        let response = update_machine_config(State(state.clone()), Form(payload)).await;
        assert_eq!(response.into_response().status(), StatusCode::SEE_OTHER);

        let machines = state.machines.read().await;
        let updated = machines.first().expect("machine should exist");
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.description.as_deref(), Some("New description"));
        assert!(updated.can_be_turned_off);
        assert_eq!(updated.request_rate.max_requests, 15);
        assert_eq!(updated.request_rate.period_minutes, 5);
        assert_eq!(updated.turn_off_port, Some(8080));
    }

    #[tokio::test]
    async fn delete_machine_stops_proxy_and_removes_machine() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp_dir = tempdir().expect("failed to create temp dir");
        let file_path = tmp_dir.path().join("machines.json");
        let _guard = EnvGuard::set_path("WAKEZILLA_MACHINES_DB_PATH", &file_path);

        let machine = sample_machine();
        let state = state_with_machines(vec![machine.clone()]);

        {
            let mut proxies = state.proxies.write().await;
            let (tx, _rx) = watch::channel(true);
            proxies.insert(format!("{}-proxy", machine.mac), tx);
        }

        let redirect = delete_machine(
            State(state.clone()),
            Form(DeleteForm {
                mac: machine.mac.clone(),
            }),
        )
        .await;
        let response = redirect.into_response();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::LOCATION)
                .and_then(|value| value.to_str().ok()),
            Some("/")
        );

        assert!(state.machines.read().await.is_empty());
        assert!(state.proxies.read().await.is_empty());
    }

    #[tokio::test]
    async fn update_ports_removes_marked_entries() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp_dir = tempdir().expect("failed to create temp dir");
        let file_path = tmp_dir.path().join("machines.json");
        let _guard = EnvGuard::set_path("WAKEZILLA_MACHINES_DB_PATH", &file_path);

        let mut machine = sample_machine();
        machine.port_forwards = vec![web::PortForward {
            name: "SSH".to_string(),
            local_port: 22,
            target_port: 22,
        }];
        let state = state_with_machines(vec![machine.clone()]);

        {
            let mut proxies = state.proxies.write().await;
            let (tx, _rx) = watch::channel(true);
            proxies.insert(format!("{}-ssh", machine.mac), tx);
        }

        let mut payload = HashMap::new();
        payload.insert("mac".to_string(), machine.mac.clone());
        payload.insert("pf_name_0".to_string(), "SSH".to_string());
        payload.insert("pf_local_0".to_string(), "22".to_string());
        payload.insert("pf_target_0".to_string(), "22".to_string());
        payload.insert("pf_remove_0".to_string(), "on".to_string());

        let response = update_ports(State(state.clone()), Form(payload)).await;
        assert_eq!(response.into_response().status(), StatusCode::SEE_OTHER);

        let machines = state.machines.read().await;
        assert!(machines[0].port_forwards.is_empty());
        assert!(state.proxies.read().await.is_empty());
    }
}
