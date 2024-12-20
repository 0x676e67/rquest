/// referrer: https://github.com/cloudflare/boring/blob/master/hyper-boring/src/lib.rs
use super::cache::{SessionCache, SessionKey};
use super::{key_index, HttpsLayerSettings, MaybeHttpsStream};
use crate::client::hyper_util::client::legacy::connect::Connection;
use crate::client::hyper_util::rt::TokioIo;
use crate::tls::{TlsConnectExtension, TlsResult};
use antidote::Mutex;
use boring::error::ErrorStack;
use boring::ssl::{
    ConnectConfiguration, Ssl, SslConnector, SslConnectorBuilder, SslRef, SslSessionCacheMode,
};
use http::uri::Scheme;
use http::Uri;
use hyper::rt::{Read, Write};
use std::error::Error;
use std::fmt;
use std::future::Future;

use std::net;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

/// A Connector using BoringSSL to support `http` and `https` schemes.
#[derive(Clone)]
pub struct HttpsConnector<T> {
    http: T,
    inner: Inner,
}

impl<S, T> HttpsConnector<S>
where
    S: Service<Uri, Response = T> + Send,
    S::Error: Into<Box<dyn Error + Send + Sync>>,
    S::Future: Unpin + Send + 'static,
    T: Read + Write + Connection + Unpin + fmt::Debug + Sync + Send + 'static,
{
    /// Creates a new `HttpsConnector` with a given `HttpConnector`
    pub fn with_connector_layer(http: S, layer: HttpsLayer) -> HttpsConnector<S> {
        HttpsConnector {
            http,
            inner: layer.inner,
        }
    }

    /// Configures the SSL context for a given URI.
    pub fn setup_ssl(&self, uri: &Uri, host: &str) -> TlsResult<Ssl> {
        self.inner.setup_ssl(uri, host)
    }

    /// Registers a callback which can customize the configuration of each connection.
    ///
    /// Unsuitable to change verify hostflags (with `config.param_mut().set_hostflags(…)`),
    /// as they are reset after the callback is executed. Use [`Self::set_ssl_callback`]
    /// instead.
    pub fn set_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut ConnectConfiguration, &Uri) -> Result<(), ErrorStack> + 'static + Sync + Send,
    {
        self.inner.callback = Some(Arc::new(callback));
    }
}

/// A layer which wraps services in an `HttpsConnector`.
#[derive(Clone)]
pub struct HttpsLayer {
    inner: Inner,
}

#[derive(Clone)]
struct Inner {
    ssl: SslConnector,
    cache: Option<Arc<Mutex<SessionCache>>>,
    callback: Option<Callback>,
    ssl_callback: Option<SslCallback>,
    skip_session_ticket: bool,
}

type Callback =
    Arc<dyn Fn(&mut ConnectConfiguration, &Uri) -> Result<(), ErrorStack> + Sync + Send>;
type SslCallback = Arc<dyn Fn(&mut SslRef, &Uri) -> Result<(), ErrorStack> + Sync + Send>;

impl HttpsLayer {
    /// Creates a new `HttpsLayer` with settings
    pub fn with_connector_and_settings(
        mut ssl: SslConnectorBuilder,
        settings: HttpsLayerSettings,
    ) -> TlsResult<HttpsLayer> {
        // If the session cache is disabled, we don't need to set up any callbacks.
        let cache = if settings.session_cache {
            let cache = Arc::new(Mutex::new(SessionCache::with_capacity(
                settings.session_cache_capacity,
            )));

            ssl.set_session_cache_mode(SslSessionCacheMode::CLIENT);

            ssl.set_new_session_callback({
                let cache = cache.clone();
                move |ssl, session| {
                    if let Some(key) = key_index().ok().and_then(|idx| ssl.ex_data(idx)) {
                        cache.lock().insert(key.clone(), session);
                    }
                }
            });

            Some(cache)
        } else {
            None
        };

        Ok(HttpsLayer {
            inner: Inner {
                ssl: ssl.build(),
                cache,
                callback: None,
                ssl_callback: None,
                skip_session_ticket: settings.skip_session_ticket,
            },
        })
    }
}

impl<S> Layer<S> for HttpsLayer {
    type Service = HttpsConnector<S>;

    fn layer(&self, inner: S) -> HttpsConnector<S> {
        HttpsConnector {
            http: inner,
            inner: self.inner.clone(),
        }
    }
}

impl Inner {
    fn setup_ssl(&self, uri: &Uri, host: &str) -> Result<Ssl, ErrorStack> {
        let mut conf = self.ssl.configure()?;

        if let Some(ref callback) = self.callback {
            callback(&mut conf, uri)?;
        }

        let key = SessionKey {
            host: host.to_string(),
            port: uri.port_u16().unwrap_or(443),
        };

        if let Some(ref cache) = self.cache {
            if let Some(session) = cache.lock().get(&key) {
                unsafe {
                    conf.set_session(&session)?;
                }

                if self.skip_session_ticket {
                    conf.configure_skip_session_ticket()?;
                }
            }
        }

        let idx = key_index()?;
        conf.set_ex_data(idx, key);

        let mut ssl = conf.into_ssl(host)?;

        if let Some(ref ssl_callback) = self.ssl_callback {
            ssl_callback(&mut ssl, uri)?;
        }

        Ok(ssl)
    }
}

impl<T, S> Service<Uri> for HttpsConnector<S>
where
    S: Service<Uri, Response = T> + Send,
    S::Error: Into<Box<dyn Error + Send + Sync>>,
    S::Future: Unpin + Send + 'static,
    T: Read + Write + Connection + Unpin + fmt::Debug + Sync + Send + 'static,
{
    type Response = MaybeHttpsStream<T>;
    type Error = Box<dyn Error + Sync + Send>;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.http.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let is_tls_scheme = uri
            .scheme()
            .map(|s| s == &Scheme::HTTPS || s.as_str() == "wss")
            .unwrap_or(false);

        let tls_setup = if is_tls_scheme {
            Some((self.inner.clone(), uri.clone()))
        } else {
            None
        };

        let connect = self.http.call(uri);

        let f = async {
            let conn = connect.await.map_err(Into::into)?;

            let (inner, uri) = match tls_setup {
                Some((inner, uri)) => (inner, uri),
                None => return Ok(MaybeHttpsStream::Http(conn)),
            };

            let mut host = uri.host().ok_or("URI missing host")?;

            // If `host` is an IPv6 address, we must strip away the square brackets that surround
            // it (otherwise, boring will fail to parse the host as an IP address, eventually
            // causing the handshake to fail due a hostname verification error).
            if !host.is_empty() {
                let last = host.len() - 1;
                let mut chars = host.chars();

                if let (Some('['), Some(']')) = (chars.next(), chars.last()) {
                    if host[1..last].parse::<net::Ipv6Addr>().is_ok() {
                        host = &host[1..last];
                    }
                }
            }

            let ssl = inner.setup_ssl(&uri, host)?;
            let stream = tokio_boring::SslStreamBuilder::new(ssl, TokioIo::new(conn))
                .connect()
                .await?;

            Ok(MaybeHttpsStream::Https(TokioIo::new(stream)))
        };

        Box::pin(f)
    }
}
