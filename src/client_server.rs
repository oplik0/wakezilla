use axum::{
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

use crate::system;

pub async fn start(port: u16) {
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/machines/turn-off", post(turn_off_machine));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> impl IntoResponse {
    let status = serde_json::json!({ "status": "ok" });
    Json(status)
}

async fn turn_off_machine() -> impl IntoResponse {
    system::shutdown_machine();
    (
        axum::http::StatusCode::OK,
        "Shutting down this machine".to_string(),
    )
}
