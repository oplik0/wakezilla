use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tower::util::ServiceExt;
use wakezilla::connection_pool::ConnectionPool;
use wakezilla::proxy_server::build_router;
use wakezilla::web::{AppState, Machine, RequestRateConfig};

struct EnvVarGuard {
    key: &'static str,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        std::env::set_var(key, value);
        Self { key }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        std::env::remove_var(self.key);
    }
}

fn setup_state(temp_dir: &TempDir) -> (AppState, EnvVarGuard) {
    let db_path = temp_dir.path().join("machines.json");
    let guard = EnvVarGuard::set(
        "WAKEZILLA_MACHINES_DB_PATH",
        db_path.to_str().expect("temp path should be valid utf-8"),
    );

    let machines = Arc::new(RwLock::new(Vec::<Machine>::new())) as Arc<RwLock<Vec<Machine>>>;
    let proxies = Arc::new(RwLock::new(HashMap::new()));
    let state = AppState {
        machines,
        proxies,
        connection_pool: ConnectionPool::new(),
    };

    (state, guard)
}

fn sample_machine() -> Machine {
    Machine {
        mac: "AA:BB:CC:DD:EE:FF".to_string(),
        ip: "127.0.0.1".parse().expect("valid ip"),
        name: "Workstation".to_string(),
        description: Some("Primary workstation".to_string()),
        turn_off_port: None,
        can_be_turned_off: false,
        request_rate: RequestRateConfig {
            max_requests: 0,
            period_minutes: 60,
        },
        port_forwards: Vec::new(),
    }
}

#[tokio::test]
async fn get_machine_detail_renders_existing_machine() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let (state, _guard) = setup_state(&temp_dir);

    {
        let mut machines = state.machines.write().await;
        machines.push(sample_machine());
    }

    let app = build_router(state.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/machines/AA:BB:CC:DD:EE:FF")
                .method("GET")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await
        .expect("handler failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let body_str = String::from_utf8(body.to_vec()).expect("body should be utf8");
    assert!(body_str.contains("Workstation"));

    let not_found = app
        .oneshot(
            Request::builder()
                .uri("/machines/11:22:33:44:55:66")
                .method("GET")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await
        .expect("handler failed");

    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wake_endpoint_rejects_invalid_mac() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let (state, _guard) = setup_state(&temp_dir);
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/wol")
                .method("POST")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("mac=invalid-mac"))
                .expect("failed to build wake request"),
        )
        .await
        .expect("wake handler failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let body_str = String::from_utf8(body.to_vec()).expect("body should be utf8");
    assert!(body_str.contains("Invalid MAC address"));
}

#[tokio::test]
async fn add_and_delete_machine_via_forms_update_state() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let (state, _guard) = setup_state(&temp_dir);
    let app = build_router(state.clone());

    let add_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/machines")
                .method("POST")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from(
                    "mac=11%3A22%3A33%3A44%3A55%3A66&ip=192.168.1.10&name=New+Machine",
                ))
                .expect("failed to build add-machine request"),
        )
        .await
        .expect("add-machine handler failed");

    assert_eq!(add_response.status(), StatusCode::SEE_OTHER);

    {
        let machines = state.machines.read().await;
        assert!(machines.iter().any(|m| m.mac == "11:22:33:44:55:66"));
    }

    let delete_response = app
        .oneshot(
            Request::builder()
                .uri("/machines/delete")
                .method("POST")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("mac=11%3A22%3A33%3A44%3A55%3A66"))
                .expect("failed to build delete-machine request"),
        )
        .await
        .expect("delete-machine handler failed");

    assert_eq!(delete_response.status(), StatusCode::SEE_OTHER);

    {
        let machines = state.machines.read().await;
        assert!(!machines.iter().any(|m| m.mac == "11:22:33:44:55:66"));
    }
}

#[tokio::test]
async fn add_machine_with_invalid_data_returns_errors() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let (state, _guard) = setup_state(&temp_dir);
    let app = build_router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/machines")
                .method("POST")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("mac=bad&ip=not-an-ip&name="))
                .expect("failed to build add-machine request"),
        )
        .await
        .expect("add-machine handler failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let html = String::from_utf8(body.to_vec()).expect("body should be utf8");
    assert!(html.contains("Invalid MAC address"));
    assert!(html.contains("Invalid IP address"));

    let machines = state.machines.read().await;
    assert!(machines.is_empty());
}
