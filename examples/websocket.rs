//! Run websocket server
//!
//! ```not_rust
//! git clone https://github.com/tokio-rs/axum && cd axum
//! cargo run -p example-websockets-http2
//! ```

use futures_util::{SinkExt, StreamExt, TryStreamExt};
use http::header;
use rquest::{Client, Impersonate, Message, RequestBuilder, Utf8Bytes};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    // Build a client to impersonate Firefox133
    let client = Client::builder()
        .impersonate(Impersonate::Firefox133)
        .danger_accept_invalid_certs(true)
        .build()?;

    // Use the API you're already familiar with
    let websocket = client
        .websocket("wss://echo.websocket.org")
        .configure_request(configure_request)
        .send()
        .await?;

    assert_eq!(websocket.version(), http::Version::HTTP_11);

    let (mut tx, mut rx) = websocket.into_websocket().await?.split();

    tokio::spawn(async move {
        for i in 1..11 {
            if let Err(err) = tx
                .send(Message::Text(Utf8Bytes::from(format!(
                    "Hello, World! #{i}"
                ))))
                .await
            {
                eprintln!("failed to send message: {err}");
            }
        }
    });

    while let Some(message) = rx.try_next().await? {
        if let Message::Text(text) = message {
            println!("received: {text}");
        }
    }

    Ok(())
}

/// We can also set HTTP options here
fn configure_request(builder: RequestBuilder) -> RequestBuilder {
    builder
        .header(header::USER_AGENT, env!("CARGO_PKG_NAME"))
        .timeout(Duration::from_secs(10))
}
