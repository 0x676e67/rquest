use std::sync::Arc;

use rquest::{
    cookie::{CookieStore, Jar},
    redirect::Policy,
    Impersonate,
};
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("trace"));

    let url = Url::parse("https://google.com/")?;

    // Build a client to impersonate Safari18
    let client = rquest::Client::builder()
        .impersonate(Impersonate::Safari18)
        .build()?;

    // Create a cookie store
    // Used to store cookies for specific multiple requests without using the client's cookie store
    let jar = Arc::new(Jar::default());

    // Make a request
    let _ = client
        .get(&url)
        .redirect(Policy::default())
        .cookie_store(jar.clone())
        .send()
        .await?;

    // Print cookies
    let cookies = jar.cookies(&url);
    log::info!("{:?}", cookies);

    // Add a cookie
    jar.add_cookie_str("foo=bar; Domain=google.com", &url);

    // Make a request
    let _ = client.get(&url).cookie_store(jar.clone()).send().await?;

    // Print cookies
    let cookies = jar.cookies(&url);
    log::info!("{:?}", cookies);

    Ok(())
}
