use crate::{Http1Config, Http2Config, TlsConfig};
use http::{HeaderMap, HeaderName};
use std::borrow::Cow;

/// Trait defining the interface for providing an `EmulationProvider`.
///
/// The `EmulationProviderFactory` trait is designed to be implemented by types that can provide
/// an `EmulationProvider` instance. This trait abstracts the creation and configuration of
/// `EmulationProvider`, allowing different types to offer their own specific configurations.
///
/// # Example
///
/// ```rust
/// use rquest::{EmulationProviderFactory, EmulationProvider};
///
/// struct MyEmulationProvider;
///
/// impl EmulationProviderFactory for MyEmulationProvider {
///     fn emulation(self) -> EmulationProvider {
///         EmulationProvider::default()
///     }
/// }
///
/// let provider = MyEmulationProvider.emulation();
/// ```
pub trait EmulationProviderFactory {
    /// Provides an `EmulationProvider` instance.
    fn emulation(self) -> EmulationProvider;
}

/// Builder for creating an `EmulationProvider`.
#[derive(Debug)]
pub struct EmulationProviderBuilder {
    provider: EmulationProvider,
}

/// HTTP connection context that manages both HTTP and TLS configurations.
///
/// The `EmulationProvider` provides a complete environment for HTTP connections,
/// including both HTTP-specific settings and the underlying TLS configuration.
/// This unified context ensures consistent behavior across connections.
///
/// # Components
///
/// - **TLS Configuration**: Manages secure connection settings.
/// - **HTTP Settings**: Controls HTTP/1 and HTTP/2 behaviors.
/// - **Header Management**: Handles default headers and their ordering.
///
/// # Example
///
/// ```rust
/// use rquest::EmulationProvider;
/// use rquest::TlsConfig;
///
/// let provider = EmulationProvider::builder()
///     .tls_config(TlsConfig::default())
///     .build();
/// ```
#[derive(Default, Debug)]
pub struct EmulationProvider {
    pub(crate) tls_config: Option<TlsConfig>,
    pub(crate) http1_config: Option<Http1Config>,
    pub(crate) http2_config: Option<Http2Config>,
    pub(crate) default_headers: Option<HeaderMap>,
    pub(crate) headers_order: Option<Cow<'static, [HeaderName]>>,
}

impl EmulationProviderBuilder {
    /// Sets the TLS configuration for the `EmulationProvider`.
    pub fn tls_config(mut self, config: TlsConfig) -> Self {
        self.provider.tls_config = Some(config);
        self
    }

    /// Sets the HTTP/1 configuration for the `EmulationProvider`.
    pub fn http1_config(mut self, config: Http1Config) -> Self {
        self.provider.http1_config = Some(config);
        self
    }

    /// Sets the HTTP/2 configuration for the `EmulationProvider`.
    pub fn http2_config(mut self, config: Http2Config) -> Self {
        self.provider.http2_config = Some(config);
        self
    }

    /// Sets the default headers for the `EmulationProvider`.
    pub fn default_headers(mut self, headers: HeaderMap) -> Self {
        self.provider.default_headers = Some(headers);
        self
    }

    /// Sets the order of headers for the `EmulationProvider`.
    pub fn headers_order<O>(mut self, order: O) -> Self
    where
        O: Into<Cow<'static, [HeaderName]>>,
    {
        self.provider.headers_order = Some(order.into());
        self
    }

    /// Builds the `EmulationProvider` instance.
    pub fn build(self) -> EmulationProvider {
        self.provider
    }
}

impl EmulationProvider {
    /// Creates a new `EmulationProviderBuilder`.
    ///
    /// # Returns
    ///
    /// Returns a new `EmulationProviderBuilder` instance.
    pub fn builder() -> EmulationProviderBuilder {
        EmulationProviderBuilder {
            provider: EmulationProvider::default(),
        }
    }
}

/// Implement `EmulationProviderFactory` for `EmulationProvider`.
///
/// This implementation allows an `EmulationProvider` to be used wherever an
/// `EmulationProviderFactory` is required, providing a default emulation configuration.
impl EmulationProviderFactory for EmulationProvider {
    fn emulation(self) -> EmulationProvider {
        self
    }
}
