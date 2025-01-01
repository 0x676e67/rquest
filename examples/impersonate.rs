use rquest::Impersonate;

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // Build a client to mimic Firefox117
    let client = rquest::Client::builder()
        .impersonate(Impersonate::Firefox117)
        .build()?;

    let resp = client.get("https://tls.peet.ws/api/all").send().await?;
    println!("{}", resp.text().await?);
    Ok(())
}
