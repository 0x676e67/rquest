use http::{header, HeaderName};
use rquest::{Client, Impersonate};

const HEADER_ORDER: &[HeaderName] = &[
    header::ACCEPT_LANGUAGE,
    header::USER_AGENT,
    header::ACCEPT_ENCODING,
    header::HOST,
    header::COOKIE,
];

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // Build a client to mimic Chrome131
    let mut client = Client::builder()
        .impersonate(Impersonate::Chrome131)
        .build()?;

    #[cfg(any(
        target_os = "android",
        target_os = "fuchsia",
        target_os = "linux",
        target_os = "ios",
        target_os = "visionos",
        target_os = "macos",
        target_os = "tvos",
        target_os = "watchos"
    ))]
    client
        .set_impersonate(Impersonate::Safari18)?
        .set_headers_order(HEADER_ORDER)
        .set_interface("utun4")
        .set_base_url("https://tls.peet.ws");

    let text = client.get("/api/all").send().await?.text().await?;

    println!("{}", text);

    Ok(())
}
