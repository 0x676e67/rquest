use super::http2::{NEW_HEADERS_PSEUDO_ORDER, NEW_HEADER_PRORIORITY, NEW_SETTINGS_ORDER};
use super::tls::{SafariTlsSettings, CIPHER_LIST};
use crate::tls::impersonate::{http2::Http2Settings, ImpersonateSettings};
use crate::tls::TlsResult;
use http::{
    header::{ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, USER_AGENT},
    HeaderMap, HeaderValue,
};

pub(crate) fn get_settings() -> TlsResult<ImpersonateSettings> {
    Ok(ImpersonateSettings::builder()
        .tls(
            SafariTlsSettings::builder()
                .cipher_list(&CIPHER_LIST)
                .build()
                .try_into()?,
        )
        .http2(
            Http2Settings::builder()
                .initial_stream_window_size(2097152)
                .initial_connection_window_size(10485760)
                .max_concurrent_streams(100)
                .enable_push(false)
                .unknown_setting8(true)
                .unknown_setting9(true)
                .headers_priority(*NEW_HEADER_PRORIORITY)
                .headers_pseudo_order(*NEW_HEADERS_PSEUDO_ORDER)
                .settings_order(NEW_SETTINGS_ORDER.to_vec())
                .build(),
        )
        .headers(Box::new(header_initializer))
        .build())
}

fn header_initializer(headers: &mut HeaderMap) {
    headers.insert("sec-fetch-dest", HeaderValue::from_static("document"));
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    headers.insert("sec-fetch-site", HeaderValue::from_static("none"));
    headers.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert("priority", HeaderValue::from_static("u=0, i"));
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br"),
    );
}
