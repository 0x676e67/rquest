use rquest::tls::KeyLogPolicy;

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // Build a client
    let client = rquest::Client::builder()
        .keylog_policy(KeyLogPolicy::File("keylog.txt".into()))
        .cert_verification(false)
        .build()?;

    // Use the API you're already familiar with
    let resp = client.get("https://tls.peet.ws/api/all").send().await?;
    println!("{}", resp.text().await?);
    Ok(())
}
