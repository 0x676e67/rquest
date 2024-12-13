use super::impersonate_imports::*;
use crate::{edge_mod_generator, tls::Http2Settings};
use http2::{HEADERS_PSEUDO_ORDER, HEADER_PRIORITY, SETTINGS_ORDER};
use tls::*;

// ============== Header initializer ==============
#[inline]
fn header_initializer(sec_ch_ua: &'static str, ua: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    header_chrome_edge_sec_ch_ua!(headers, sec_ch_ua);
    header_chrome_edge_ua!(headers, ua);
    header_chrome_edge_sec_fetch!(headers);
    header_chrome_edge_accpet!(headers);
    headers
}

#[inline]
fn header_initializer_with_zstd_priority(sec_ch_ua: &'static str, ua: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    header_chrome_edge_sec_ch_ua!(headers, sec_ch_ua);
    header_chrome_edge_ua!(headers, ua);
    header_chrome_edge_sec_fetch!(headers);
    header_chrome_edge_accpet_with_zstd!(headers);
    headers.insert("priority", HeaderValue::from_static("u=0, i"));
    headers
}

// ============== TLS settings ==============
mod tls {
    use crate::tls::impersonate::tls_imports::*;

    pub const CURVES: &[SslCurve] = &[SslCurve::X25519, SslCurve::SECP256R1, SslCurve::SECP384R1];

    pub const NEW_CURVES: &[SslCurve] = &[
        SslCurve::X25519_KYBER768_DRAFT00,
        SslCurve::X25519,
        SslCurve::SECP256R1,
        SslCurve::SECP384R1,
    ];

    pub const CIPHER_LIST: &str = static_join!(
        ":",
        "TLS_AES_128_GCM_SHA256",
        "TLS_AES_256_GCM_SHA384",
        "TLS_CHACHA20_POLY1305_SHA256",
        "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
        "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
        "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
        "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
        "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
        "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
        "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
        "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
        "TLS_RSA_WITH_AES_128_GCM_SHA256",
        "TLS_RSA_WITH_AES_256_GCM_SHA384",
        "TLS_RSA_WITH_AES_128_CBC_SHA",
        "TLS_RSA_WITH_AES_256_CBC_SHA"
    );

    pub const SIGALGS_LIST: &str = static_join!(
        ":",
        "ecdsa_secp256r1_sha256",
        "rsa_pss_rsae_sha256",
        "rsa_pkcs1_sha256",
        "ecdsa_secp384r1_sha384",
        "rsa_pss_rsae_sha384",
        "rsa_pkcs1_sha384",
        "rsa_pss_rsae_sha512",
        "rsa_pkcs1_sha512"
    );

    #[derive(TypedBuilder)]
    pub struct EdgeTlsSettings {
        // TLS curves
        #[builder(default = CURVES)]
        curves: &'static [SslCurve],

        // TLS sigalgs list
        #[builder(default = SIGALGS_LIST)]
        sigalgs_list: &'static str,

        // TLS cipher list
        #[builder(default = CIPHER_LIST)]
        cipher_list: &'static str,

        // TLS application_settings extension
        #[builder(default = true, setter(into))]
        application_settings: bool,

        // TLS enable ech grease, https://chromestatus.com/feature/6196703843581952
        #[builder(default = false, setter(into))]
        enable_ech_grease: bool,

        // TLS permute extensions
        #[builder(default = false, setter(into))]
        permute_extensions: bool,

        // TLS pre_shared_key extension
        #[builder(default = false, setter(into))]
        pre_shared_key: bool,
    }

    impl From<EdgeTlsSettings> for TlsSettings {
        fn from(val: EdgeTlsSettings) -> Self {
            TlsSettings::builder()
                .grease_enabled(true)
                .enable_ocsp_stapling(true)
                .enable_signed_cert_timestamps(true)
                .curves(Cow::Borrowed(val.curves))
                .sigalgs_list(Cow::Borrowed(val.sigalgs_list))
                .cipher_list(Cow::Borrowed(val.cipher_list))
                .min_tls_version(TlsVersion::TLS_1_2)
                .max_tls_version(TlsVersion::TLS_1_3)
                .permute_extensions(val.permute_extensions)
                .pre_shared_key(val.pre_shared_key)
                .enable_ech_grease(val.enable_ech_grease)
                .application_settings(val.application_settings)
                .cert_compression_algorithm(CertCompressionAlgorithm::Brotli)
                .build()
        }
    }

    #[macro_export]
    macro_rules! edge_tls_template {
        (1) => {{
            super::EdgeTlsSettings::builder().build().into()
        }};
        (2) => {{
            super::EdgeTlsSettings::builder()
                .permute_extensions(true)
                .pre_shared_key(true)
                .enable_ech_grease(true)
                .build()
                .into()
        }};
        (3, $curves:expr) => {{
            super::EdgeTlsSettings::builder()
                .curves($curves)
                .permute_extensions(true)
                .pre_shared_key(true)
                .enable_ech_grease(true)
                .build()
                .into()
        }};
    }
}

// ============== Http2 settings ==============
mod http2 {
    use crate::tls::impersonate::http2_imports::*;

    // ============== http2 headers priority ==============
    pub const HEADER_PRIORITY: (u32, u8, bool) = (0, 255, true);

    /// ============== http2 headers pseudo order ==============
    pub const HEADERS_PSEUDO_ORDER: [PseudoOrder; 4] = [Method, Authority, Scheme, Path];

    /// ============== http2 settings frame order ==============
    pub static SETTINGS_ORDER: [SettingsOrder; 8] = [
        HeaderTableSize,
        EnablePush,
        MaxConcurrentStreams,
        InitialWindowSize,
        MaxFrameSize,
        MaxHeaderListSize,
        UnknownSetting8,
        UnknownSetting9,
    ];

    #[macro_export]
    macro_rules! edge_http2_template {
        (1) => {{
            super::Http2Settings::builder()
                .initial_stream_window_size(6291456)
                .initial_connection_window_size(15728640)
                .max_concurrent_streams(1000)
                .max_header_list_size(262144)
                .header_table_size(65536)
                .headers_priority(super::HEADER_PRIORITY)
                .headers_pseudo_order(super::HEADERS_PSEUDO_ORDER)
                .settings_order(super::SETTINGS_ORDER)
                .build()
        }};
        (2) => {{
            super::Http2Settings::builder()
                .initial_stream_window_size(6291456)
                .initial_connection_window_size(15728640)
                .max_header_list_size(262144)
                .header_table_size(65536)
                .enable_push(false)
                .headers_priority(super::HEADER_PRIORITY)
                .headers_pseudo_order(super::HEADERS_PSEUDO_ORDER)
                .settings_order(super::SETTINGS_ORDER)
                .build()
        }};
    }
}

edge_mod_generator!(
    edge101,
    edge_tls_template!(1),
    edge_http2_template!(1),
    header_initializer,
    r#""Not A;Brand";v="99", "Chromium";v="101", "Microsoft Edge";v="101""#,
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/101.0.4951.64 Safari/537.36 Edg/101.0.1210.47"
);

edge_mod_generator!(
    edge122,
    edge_tls_template!(2),
    edge_http2_template!(2),
    header_initializer,
    "\"Chromium\";v=\"122\", \"Not(A:Brand\";v=\"24\", \"Microsoft Edge\";v=\"122\"",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 Edg/122.0.0.0"
);

edge_mod_generator!(
    edge127,
    edge_tls_template!(3, NEW_CURVES),
    edge_http2_template!(2),
    header_initializer_with_zstd_priority,
    "\"Not)A;Brand\";v=\"99\", \"Microsoft Edge\";v=\"127\", \"Chromium\";v=\"127\"",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36 Edg/127.0.0.0"
);
