use axum::{
    extract::Form,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::net::TcpListener;

use crate::wol;

#[derive(Deserialize)]
pub struct WolForm {
    mac: String,
    ip: Option<Ipv4Addr>,
    port: Option<u16>,
}

pub async fn run() {
    let app = Router::new()
        .route("/", get(root))
        .route("/wol", post(wol_post));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> Html<&'static str> {
    Html(
        r#"
        <!doctype html>
        <html>
            <head>
                <title>Wake-on-LAN</title>
            </head>
            <body>
                <h1>Wake-on-LAN</h1>
                <form action="/wol" method="post">
                    <label for="mac">MAC Address:</label><br>
                    <input type="text" id="mac" name="mac" required size="50"><br>
                    <small>Formats: 00:11:22:33:44:55, 00-11-22-33-44-55, 001122334455</small><br><br>
                    
                    <label for="ip">Broadcast IP (optional):</label><br>
                    <input type="text" id="ip" name="ip" size="50"><br>
                    <small>Default: 255.255.255.255</small><br><br>

                    <label for="port">Port (optional):</label><br>
                    <input type="number" id="port" name="port"><br>
                    <small>Default: 9</small><br><br>

                    <input type="submit" value="Send WOL Packet">
                </form>
            </body>
        </html>
    "#,
    )
}

async fn wol_post(Form(payload): Form<WolForm>) -> impl IntoResponse {
    let mac = match wol::parse_mac(&payload.mac) {
        Ok(mac) => mac,
        Err(e) => {
            return format!("Invalid MAC address '{}': {}", payload.mac, e);
        }
    };

    let bcast = payload.ip.unwrap_or(Ipv4Addr::new(255, 255, 255, 255));
    let port = payload.port.unwrap_or(9);
    let count = 3;

    match wol::send_packets(&mac, bcast, port, count) {
        Ok(_) => format!("Sent WOL packet to {}", payload.mac),
        Err(e) => format!("Failed to send WOL packet: {}", e),
    }
}
