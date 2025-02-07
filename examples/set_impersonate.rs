use rquest::{Client, Impersonate};

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // Build a client to impersonate Chrome131
    let client = Client::builder()
        .impersonate(Impersonate::Chrome131)
        .build()?;

    // Change the impersonate to Safari18
    client
        .client_mut()
        .impersonate(Impersonate::Safari18)
        .apply()?;
    let resp = client.get("https://tls.peet.ws/api/all").send().await?;
    println!("{}", resp.text().await?);

    Ok(())
}
