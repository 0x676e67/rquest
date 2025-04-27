use http::HeaderValue;
use rquest::cookie::Jar;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // Build a client
    let client = rquest::Client::new();

    let url = "https://tls.peet.ws/api/all".parse().expect("Invalid url");

    // Set cookie provider
    client
        .update()
        .cookie_provider(Arc::new(Jar::default()))
        .apply()?;

    // Set a cookie
    client.set_cookie(&url, HeaderValue::from_static("foo=bar"));

    // Use the API you're already familiar with
    let resp = client.get(url).send().await?.text().await?;
    println!("{}", resp);

    Ok(())
}
