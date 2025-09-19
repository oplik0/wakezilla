use std::io::ErrorKind;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use wakezilla::forward;

#[tokio::test]
async fn turn_off_remote_machine_sends_post_request() {
}
