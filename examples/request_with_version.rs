use http::Version;

#[tokio::main]
async fn main() -> wreq::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    let resp = wreq::Client::new()
        .get("https://tls.peet.ws/api/all")
        .version(Version::HTTP_11)
        .send()
        .await?;

    assert_eq!(resp.version(), Version::HTTP_11);
    println!("{}", resp.text().await?);

    Ok(())
}
