use crate::tls::Http2Settings;
use http::{
    header::{ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, USER_AGENT},
    HeaderMap, HeaderValue,
};
use http2::{HEADERS_PSEUDO_ORDER, HEADER_PRIORITY, SETTINGS_ORDER};

// ============== Headers ==============
#[inline]
fn header_initializer(ua: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(USER_AGENT, HeaderValue::from_static(ua));
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br"),
    );
    headers
}

// ============== TLS settings ==============
mod tls {
    use crate::tls::impersonate::tls_imports::*;

    pub const CURVES: &[SslCurve] = &[SslCurve::X25519, SslCurve::SECP256R1, SslCurve::SECP384R1];

    pub const SIGALGS_LIST: &str = static_join!(
        ":",
        "ecdsa_secp256r1_sha256",
        "rsa_pss_rsae_sha256",
        "rsa_pkcs1_sha256",
        "ecdsa_secp384r1_sha384",
        "rsa_pss_rsae_sha384",
        "rsa_pkcs1_sha384",
        "rsa_pss_rsae_sha512",
        "rsa_pkcs1_sha512",
        "rsa_pkcs1_sha1"
    );

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
        "TLS_RSA_WITH_AES_256_CBC_SHA",
        "TLS_RSA_WITH_3DES_EDE_CBC_SHA"
    );

    #[derive(TypedBuilder)]
    pub struct OkHttpTlsSettings {
        // TLS curves
        #[builder(default = CURVES)]
        curves: &'static [SslCurve],

        // TLS sigalgs list
        #[builder(default = SIGALGS_LIST)]
        sigalgs_list: &'static str,

        // TLS cipher list
        cipher_list: &'static str,
    }

    impl Into<TlsSettings> for OkHttpTlsSettings {
        fn into(self) -> TlsSettings {
            TlsSettings::builder()
                .enable_ocsp_stapling(true)
                .curves(Cow::Borrowed(self.curves))
                .sigalgs_list(Cow::Borrowed(self.sigalgs_list))
                .cipher_list(Cow::Borrowed(self.cipher_list))
                .min_tls_version(TlsVersion::TLS_1_2)
                .max_tls_version(TlsVersion::TLS_1_3)
                .build()
        }
    }

    #[macro_export]
    macro_rules! okhttp_tls_template {
        ($cipher_list:expr) => {
            OkHttpTlsSettings::builder()
                .cipher_list($cipher_list)
                .build()
                .into()
        };
    }
}

// ============== Http2 settings ==============
mod http2 {
    use crate::tls::impersonate::http2_imports::*;

    // ============== http2 headers priority ==============
    pub const HEADER_PRIORITY: (u32, u8, bool) = (0, 255, true);

    /// ============== http2 headers pseudo order ==============
    pub const HEADERS_PSEUDO_ORDER: [PseudoOrder; 4] = [Method, Path, Authority, Scheme];

    /// ============== http2 settings frame order ==============
    pub const SETTINGS_ORDER: [SettingsOrder; 8] = [
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
    macro_rules! okhttp_http2_template {
        () => {
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
        };
    }
}

pub(crate) mod okhttp3_11 {
    use super::tls::OkHttpTlsSettings;
    use crate::tls::{impersonate::impersonate_imports::*, okhttp::header_initializer};

    #[inline]
    pub fn get_settings(with_headers: bool) -> ImpersonateSettings {
        let tls = okhttp_tls_template!(static_join!(
            ":",
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
            "TLS_RSA_WITH_AES_256_CBC_SHA",
            "TLS_RSA_WITH_3DES_EDE_CBC_SHA"
        ));

        let headers = conditional_headers!(with_headers, || {
            header_initializer("NRC Audio/2.0.6 (nl.nrc.audio; build:36; Android 12; Sdk:31; Manufacturer:motorola; Model: moto g72) OkHttp/3.11.0")
        });

        ImpersonateSettings::builder()
            .tls(tls)
            .http2(okhttp_http2_template!())
            .headers(headers)
            .build()
    }
}

pub(crate) mod okhttp3_13 {
    use super::tls::OkHttpTlsSettings;
    use crate::tls::{impersonate::impersonate_imports::*, okhttp::header_initializer};

    #[inline]
    pub fn get_settings(with_headers: bool) -> ImpersonateSettings {
        let tls = okhttp_tls_template!(static_join!(
            ":",
            "TLS_AES_128_GCM_SHA256",
            "TLS_AES_256_GCM_SHA384",
            "TLS_CHACHA20_POLY1305_SHA256",
            "TLS_AES_128_CCM_SHA256",
            "TLS_AES_256_CCM_8_SHA256",
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
            "TLS_RSA_WITH_AES_256_CBC_SHA",
            "TLS_RSA_WITH_3DES_EDE_CBC_SHA"
        ));

        let headers = conditional_headers!(with_headers, || {
            header_initializer("GM-Android/6.112.2 (240590300; M:Google Pixel 7a; O:34; D:2b045e03986fa6dc) ObsoleteUrlFactory/1.0 OkHttp/3.13.0")
        });

        ImpersonateSettings::builder()
            .tls(tls)
            .http2(okhttp_http2_template!())
            .headers(headers)
            .build()
    }
}

pub(crate) mod okhttp3_14 {
    use super::tls::{OkHttpTlsSettings, CIPHER_LIST};
    use crate::tls::{impersonate::impersonate_imports::*, okhttp::header_initializer};

    #[inline]
    pub fn get_settings(with_headers: bool) -> ImpersonateSettings {
        let headers = conditional_headers!(with_headers, || {
            header_initializer("DS podcast/2.0.1 (be.standaard.audio; build:9; Android 11; Sdk:30; Manufacturer:samsung; Model: SM-A405FN) OkHttp/3.14.0")
        });

        ImpersonateSettings::builder()
            .tls(okhttp_tls_template!(CIPHER_LIST))
            .http2(okhttp_http2_template!())
            .headers(headers)
            .build()
    }
}

pub(crate) mod okhttp3_9 {
    use super::tls::OkHttpTlsSettings;
    use crate::tls::{impersonate::impersonate_imports::*, okhttp::header_initializer};

    #[inline]
    pub fn get_settings(with_headers: bool) -> ImpersonateSettings {
        let tls = okhttp_tls_template!(static_join!(
            ":",
            "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
            "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
            "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
            "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
            "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
            "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA",
            "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
            "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA",
            "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
            "TLS_RSA_WITH_AES_128_GCM_SHA256",
            "TLS_RSA_WITH_AES_256_GCM_SHA384",
            "TLS_RSA_WITH_AES_128_CBC_SHA",
            "TLS_RSA_WITH_AES_256_CBC_SHA",
            "TLS_RSA_WITH_3DES_EDE_CBC_SHA"
        ));

        let headers = conditional_headers!(with_headers, || {
            header_initializer("MaiMemo/4.4.50_639 okhttp/3.9 Android/5.0 Channel/WanDouJia Device/alps+M8+Emulator (armeabi-v7a) Screen/4.44 Resolution/480x800 DId/aa6cde19def3806806d5374c4e5fd617 RAM/0.94 ROM/4.91 Theme/Day")
        });

        ImpersonateSettings::builder()
            .tls(tls)
            .http2(okhttp_http2_template!())
            .headers(headers)
            .build()
    }
}

pub(crate) mod okhttp4_10 {
    use super::tls::{OkHttpTlsSettings, CIPHER_LIST};
    use crate::tls::{impersonate::impersonate_imports::*, okhttp::header_initializer};

    #[inline]
    pub fn get_settings(with_headers: bool) -> ImpersonateSettings {
        let headers = conditional_headers!(with_headers, || {
            header_initializer("GM-Android/6.112.2 (240590300; M:samsung SM-G781U1; O:33; D:edb34792871638d8) ObsoleteUrlFactory/1.0 OkHttp/4.10.0")
        });

        ImpersonateSettings::builder()
            .tls(okhttp_tls_template!(CIPHER_LIST))
            .http2(okhttp_http2_template!())
            .headers(headers)
            .build()
    }
}

pub(crate) mod okhttp4_9 {
    use super::tls::OkHttpTlsSettings;
    use crate::tls::{impersonate::impersonate_imports::*, okhttp::header_initializer};

    #[inline]
    pub fn get_settings(with_headers: bool) -> ImpersonateSettings {
        let tls = okhttp_tls_template!(static_join!(
            ":",
            "TLS_AES_128_GCM_SHA256",
            "TLS_AES_256_GCM_SHA384",
            "TLS_CHACHA20_POLY1305_SHA256",
            "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
            "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
            "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
            "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
            "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
            "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
            "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
            "TLS_RSA_WITH_AES_128_GCM_SHA256",
            "TLS_RSA_WITH_AES_256_GCM_SHA384",
            "TLS_RSA_WITH_AES_128_CBC_SHA",
            "TLS_RSA_WITH_AES_256_CBC_SHA"
        ));

        let headers = conditional_headers!(with_headers, || {
            header_initializer("GM-Android/6.111.1 (240460200; M:motorola moto g power (2021); O:30; D:76ba9f6628d198c8) ObsoleteUrlFactory/1.0 OkHttp/4.9")
        });

        ImpersonateSettings::builder()
            .tls(tls)
            .http2(okhttp_http2_template!())
            .headers(headers)
            .build()
    }
}

pub(crate) mod okhttp5 {
    use super::tls::{OkHttpTlsSettings, CIPHER_LIST};
    use crate::tls::{impersonate::impersonate_imports::*, okhttp::header_initializer};

    #[inline]
    pub fn get_settings(with_headers: bool) -> ImpersonateSettings {
        let headers = conditional_headers!(with_headers, || {
            header_initializer("NRC Audio/2.0.6 (nl.nrc.audio; build:36; Android 14; Sdk:34; Manufacturer:OnePlus; Model: CPH2609) OkHttp/5.0.0-alpha2")
        });

        ImpersonateSettings::builder()
            .tls(okhttp_tls_template!(CIPHER_LIST))
            .http2(okhttp_http2_template!())
            .headers(headers)
            .build()
    }
}
