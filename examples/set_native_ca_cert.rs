use boring::x509::{store::X509StoreBuilder, X509};
use rquest::tls::Impersonate;

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // Build a client to mimic Edge127
    let client = rquest::Client::builder()
        .impersonate(Impersonate::Edge127)
        .ca_cert_store(|| {
            let mut verify_store = X509StoreBuilder::new()?;
            for cert in rustls_native_certs::load_native_certs().certs {
                let cert = X509::from_der(&*cert)?;
                verify_store.add_cert(cert)?;
            }
            Ok(verify_store.build())
        })
        .build()?;

    // Use the API you're already familiar with
    let resp = client.get("https://tls.peet.ws/api/all").send().await?;
    println!("{}", resp.text().await?);

    Ok(())
}
