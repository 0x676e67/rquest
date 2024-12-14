use http::{header, HeaderMap, HeaderName, HeaderValue};
use rquest::{
    tls::{Http2Settings, ImpersonateSettings, TlsSettings, TlsVersion},
    HttpVersionPref,
};
use rquest::{PseudoOrder::*, SettingsOrder::*};
use std::borrow::Cow;

static HEADER_ORDER: &[HeaderName] = &[
    header::USER_AGENT,
    header::ACCEPT_LANGUAGE,
    header::ACCEPT_ENCODING,
    header::COOKIE,
    header::HOST,
];

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // TLS settings
    let tls_settings = TlsSettings::builder()
        .tls_sni(true)
        .alpn_protos(HttpVersionPref::All)
        .application_settings(true)
        .pre_shared_key(true)
        .enable_ech_grease(true)
        .permute_extensions(true)
        .min_tls_version(TlsVersion::TLS_1_0)
        .max_tls_version(TlsVersion::TLS_1_3)
        .build();

    // HTTP/2 settings
    let http2_settings = Http2Settings::builder()
        .initial_stream_window_size(6291456)
        .initial_connection_window_size(15728640)
        .max_concurrent_streams(1000)
        .max_header_list_size(262144)
        .header_table_size(65536)
        .enable_push(false)
        .headers_priority((0, 255, true))
        .headers_pseudo_order([Method, Scheme, Authority, Path])
        .settings_order([
            HeaderTableSize,
            EnablePush,
            MaxConcurrentStreams,
            InitialWindowSize,
            MaxFrameSize,
            MaxHeaderListSize,
            UnknownSetting8,
            UnknownSetting9,
        ])
        .build();

    // Headers
    let headers = {
        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, HeaderValue::from_static("rquest"));
        headers.insert(
            header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("en-US,en;q=0.9"),
        );
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("gzip, deflate, br"),
        );
        headers.insert(header::HOST, HeaderValue::from_static("tls.peet.ws"));
        headers.insert(header::COOKIE, HeaderValue::from_static("foo=bar"));
        Cow::Owned(headers)
    };

    // Create impersonate settings
    let settings = ImpersonateSettings::builder()
        .tls(tls_settings)
        .http2(http2_settings)
        .headers(headers)
        .headers_order(Cow::Borrowed(HEADER_ORDER))
        .build();

    // Build a client with impersonate settings
    let client = rquest::Client::builder()
        .impersonate_settings(settings)
        .build()?;

    // Use the API you're already familiar with
    let resp = client.get("https://tls.peet.ws/api/all").send().await?;
    println!("{}", resp.text().await?);

    Ok(())
}
