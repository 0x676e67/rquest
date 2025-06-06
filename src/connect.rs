use self::tls_conn::BoringTlsConn;

use crate::core::client::connect::proxy::Tunnel;
use crate::core::client::{
    Dst,
    connect::{Connected, Connection},
};
use crate::core::rt::TokioIo;
use crate::core::rt::{Read, ReadBufCursor, Write};
use crate::tls::{HttpsConnector, MaybeHttpsStream, TlsConnector};

use http::uri::Scheme;
use pin_project_lite::pin_project;
use sealed::{Conn, Unnameable};
use tokio_boring2::SslStream;
use tower::util::{BoxCloneSyncServiceLayer, MapRequestLayer};
use tower::{ServiceBuilder, timeout::TimeoutLayer, util::BoxCloneSyncService};
use tower_service::Service;

use std::future::Future;
use std::io::{self, IoSlice};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::dns::DynResolver;
use crate::error::{BoxError, cast_to_internal_error};
use crate::proxy::{Intercepted, Matcher as ProxyMatcher};

pub(crate) type HttpConnector = crate::core::client::connect::HttpConnector<DynResolver>;

pub(crate) type BoxedConnectorService = BoxCloneSyncService<Unnameable, Conn, BoxError>;

pub(crate) type BoxedConnectorLayer =
    BoxCloneSyncServiceLayer<BoxedConnectorService, Unnameable, Conn, BoxError>;

pub(crate) struct ConnectorBuilder {
    http: HttpConnector,
    tls: TlsConnector,
    proxies: Arc<Vec<ProxyMatcher>>,
    verbose: verbose::Wrapper,
    timeout: Option<Duration>,
    nodelay: bool,
    tls_info: bool,
    #[cfg(feature = "socks")]
    resolver: Option<DynResolver>,
}

impl ConnectorBuilder {
    pub(crate) fn build<L>(self, layers: L) -> Connector
    where
        L: Into<Option<Vec<BoxedConnectorLayer>>>,
    {
        let base_service = ConnectorService {
            http: self.http,
            tls: self.tls,
            proxies: self.proxies,
            verbose: self.verbose,
            nodelay: self.nodelay,
            tls_info: self.tls_info,
            timeout: self.timeout,
            #[cfg(feature = "socks")]
            resolver: self.resolver.unwrap_or_else(DynResolver::gai),
        };

        match layers.into() {
            Some(layers) => {
                // otherwise we have user provided layers
                // so we need type erasure all the way through
                // as well as mapping the unnameable type of the layers back to Dst for the inner service
                let service = layers.iter().fold(
                    BoxCloneSyncService::new(
                        ServiceBuilder::new()
                            .layer(MapRequestLayer::new(|request: Unnameable| request.0))
                            .service(base_service.clone()),
                    ),
                    |service, layer| ServiceBuilder::new().layer(layer).service(service),
                );

                // now we handle the concrete stuff - any `connect_timeout`,
                // plus a final map_err layer we can use to cast default tower layer
                // errors to internal errors
                match self.timeout {
                    Some(timeout) => {
                        let service = ServiceBuilder::new()
                            .layer(TimeoutLayer::new(timeout))
                            .service(service);
                        let service = ServiceBuilder::new()
                            .map_err(cast_to_internal_error)
                            .service(service);
                        let service = BoxCloneSyncService::new(service);
                        Connector::WithLayers {
                            layers,
                            base_service,
                            service,
                        }
                    }
                    None => {
                        // no timeout, but still map err
                        // no named timeout layer but we still map errors since
                        // we might have user-provided timeout layer
                        let service = ServiceBuilder::new()
                            .map_err(cast_to_internal_error)
                            .service(service);
                        let service = BoxCloneSyncService::new(service);
                        Connector::WithLayers {
                            layers,
                            base_service,
                            service,
                        }
                    }
                }
            }
            None => {
                // we have no user-provided layers, only use concrete types
                Connector::Simple(base_service)
            }
        }
    }

    #[inline]
    pub(crate) fn keepalive(mut self, dur: Option<Duration>) -> ConnectorBuilder {
        self.http.set_keepalive(dur);
        self
    }

    #[inline]
    pub(crate) fn tcp_keepalive_interval(mut self, dur: Option<Duration>) -> ConnectorBuilder {
        self.http.set_keepalive_interval(dur);
        self
    }

    #[inline]
    pub(crate) fn tcp_keepalive_retries(mut self, retries: Option<u32>) -> ConnectorBuilder {
        self.http.set_keepalive_retries(retries);
        self
    }

    #[inline]
    pub(crate) fn timeout(mut self, timeout: Option<Duration>) -> ConnectorBuilder {
        self.timeout = timeout;
        self.http.set_connect_timeout(timeout);
        self
    }

    #[cfg(any(
        target_os = "android",
        target_os = "fuchsia",
        target_os = "illumos",
        target_os = "ios",
        target_os = "linux",
        target_os = "macos",
        target_os = "solaris",
        target_os = "tvos",
        target_os = "visionos",
        target_os = "watchos",
    ))]
    #[inline]
    pub(crate) fn interface(
        mut self,
        iface: Option<std::borrow::Cow<'static, str>>,
    ) -> ConnectorBuilder {
        self.http.set_interface(iface);
        self
    }

    pub(crate) fn local_addresses(
        mut self,
        local_ipv4_address: Option<Ipv4Addr>,
        local_ipv6_address: Option<Ipv6Addr>,
    ) -> ConnectorBuilder {
        match (local_ipv4_address, local_ipv6_address) {
            (Some(ipv4), None) => self.http.set_local_address(Some(IpAddr::from(ipv4))),
            (None, Some(ipv6)) => self.http.set_local_address(Some(IpAddr::from(ipv6))),
            (Some(ipv4), Some(ipv6)) => {
                self.http.set_local_addresses(ipv4, ipv6);
            }
            (None, None) => {}
        }

        self
    }

    #[inline]
    pub(crate) fn verbose(mut self, enabled: bool) -> ConnectorBuilder {
        self.verbose.0 = enabled;
        self
    }

    #[cfg(feature = "socks")]
    #[inline]
    pub(crate) fn socks_resolver<R>(mut self, resolver: R) -> ConnectorBuilder
    where
        R: Into<Option<DynResolver>>,
    {
        self.resolver = resolver.into();
        self
    }
}

#[derive(Clone)]
pub(crate) enum Connector {
    // base service, with or without an embedded timeout
    Simple(ConnectorService),
    // at least one custom layer along with maybe an outer timeout layer
    // from `builder.connect_timeout()`
    WithLayers {
        layers: Vec<BoxedConnectorLayer>,
        service: BoxedConnectorService,
        base_service: ConnectorService,
    },
}

impl Connector {
    pub(crate) fn builder(
        mut http: HttpConnector,
        tls: TlsConnector,
        proxies: Arc<Vec<ProxyMatcher>>,
        nodelay: bool,
        tls_info: bool,
    ) -> ConnectorBuilder {
        http.enforce_http(false);
        ConnectorBuilder {
            http,
            tls,
            proxies,
            verbose: verbose::OFF,
            timeout: None,
            nodelay,
            tls_info,
            #[cfg(feature = "socks")]
            resolver: None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn set_tls_connector(&mut self, mut connector: TlsConnector) {
        match self {
            Connector::Simple(service) => {
                std::mem::swap(&mut service.tls, &mut connector);
            }
            Connector::WithLayers {
                layers,
                base_service,
                ..
            } => {
                let builder = Connector::builder(
                    base_service.http.clone(),
                    connector,
                    base_service.proxies.clone(),
                    base_service.nodelay,
                    base_service.tls_info,
                )
                .timeout(base_service.timeout)
                .verbose(base_service.verbose.0);

                let mut connector = {
                    #[cfg(feature = "socks")]
                    {
                        builder
                            .socks_resolver(base_service.resolver.clone())
                            .build(std::mem::take(layers))
                    }
                    #[cfg(not(feature = "socks"))]
                    {
                        builder.build(std::mem::take(layers))
                    }
                };

                std::mem::swap(self, &mut connector);
            }
        }
    }
}

impl Service<Dst> for Connector {
    type Response = Conn;
    type Error = BoxError;
    type Future = Connecting;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self {
            Connector::Simple(service) => service.poll_ready(cx),
            Connector::WithLayers { service, .. } => service.poll_ready(cx),
        }
    }

    fn call(&mut self, dst: Dst) -> Self::Future {
        match self {
            Connector::Simple(service) => service.call(dst),
            Connector::WithLayers { service, .. } => service.call(Unnameable(dst)),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ConnectorService {
    http: HttpConnector,
    tls: TlsConnector,
    proxies: Arc<Vec<ProxyMatcher>>,
    verbose: verbose::Wrapper,
    /// When there is a single timeout layer and no other layers,
    /// we embed it directly inside our base Service::call().
    /// This lets us avoid an extra `Box::pin` indirection layer
    /// since `tokio::time::Timeout` is `Unpin`
    timeout: Option<Duration>,
    nodelay: bool,
    tls_info: bool,
    #[cfg(feature = "socks")]
    resolver: DynResolver,
}

impl ConnectorService {
    #[cfg(feature = "socks")]
    async fn connect_socks(&self, mut dst: Dst, proxy: Intercepted) -> Result<Conn, BoxError> {
        let dns = match proxy.uri().scheme_str() {
            Some("socks4" | "socks5") => socks::DnsResolve::Local,
            Some("socks4a" | "socks5h") => socks::DnsResolve::Proxy,
            _ => unreachable!("connect_socks is only called for socks proxies"),
        };

        let uri = dst.uri().clone();

        if uri.scheme() == Some(&Scheme::HTTPS) {
            let http = HttpsConnector::new(self.http.clone(), self.tls.clone(), &mut dst);

            trace!("socks HTTPS over proxy");
            let host = uri.host().ok_or(crate::error::uri_bad_host())?;
            let conn = socks::connect(proxy, &uri, dns, &self.resolver).await?;
            let io = http.connect(&uri, host, TokioIo::new(conn)).await?;

            return Ok(Conn {
                inner: self.verbose.wrap(BoringTlsConn {
                    inner: TokioIo::new(io),
                }),
                is_proxy: false,
                tls_info: self.tls_info,
            });
        }

        socks::connect(proxy, &uri, dns, &self.resolver)
            .await
            .map(|tcp| Conn {
                inner: self.verbose.wrap(TokioIo::new(tcp)),
                is_proxy: false,
                tls_info: false,
            })
    }

    async fn connect_with_maybe_proxy(
        self,
        mut dst: Dst,
        is_proxy: bool,
    ) -> Result<Conn, BoxError> {
        let uri = dst.uri().clone();
        let mut http = self.http.clone();

        // Disable Nagle's algorithm for TLS handshake
        //
        // https://www.openssl.org/docs/man1.1.1/man3/SSL_connect.html#NOTES
        if !self.nodelay && (uri.scheme() == Some(&Scheme::HTTPS)) {
            http.set_nodelay(true);
        }

        trace!("connect with maybe proxy");
        let mut http = HttpsConnector::new(http, self.tls, &mut dst);
        let io = http.call(uri).await?;

        if let MaybeHttpsStream::Https(stream) = io {
            if !self.nodelay {
                stream
                    .inner()
                    .get_ref()
                    .inner()
                    .inner()
                    .set_nodelay(false)?;
            }
            Ok(Conn {
                inner: self.verbose.wrap(BoringTlsConn { inner: stream }),
                is_proxy,
                tls_info: self.tls_info,
            })
        } else {
            Ok(Conn {
                inner: self.verbose.wrap(io),
                is_proxy,
                tls_info: self.tls_info,
            })
        }
    }

    async fn connect_via_proxy(self, mut dst: Dst, proxy: Intercepted) -> Result<Conn, BoxError> {
        let uri = dst.uri().clone();
        debug!("proxy({:?}) intercepts '{:?}'", proxy, dst);

        #[cfg(feature = "socks")]
        if let Some("socks4" | "socks4a" | "socks5" | "socks5h") = proxy.uri().scheme_str() {
            return self.connect_socks(dst, proxy).await;
        }

        let proxy_dst = proxy.uri().clone();
        let auth = proxy.basic_auth().cloned();

        if uri.scheme() == Some(&Scheme::HTTPS) {
            trace!("tunneling HTTPS over proxy");
            let http = HttpsConnector::new(self.http.clone(), self.tls, &mut dst);

            let mut tunnel = Tunnel::new(proxy_dst, http.clone());
            if let Some(auth) = auth {
                tunnel = tunnel.with_auth(auth);
            }

            if let Some(headers) = proxy.custom_headers() {
                tunnel = tunnel.with_headers(headers.clone());
            }

            let host = uri.host().ok_or(crate::error::uri_bad_host())?;

            // We don't wrap this again in an HttpsConnector since that uses Maybe,
            // and we know this is definitely HTTPS.
            let tunneled = tunnel.call(uri.clone()).await?;
            let io = http.connect(&uri, host, tunneled).await?;

            return Ok(Conn {
                inner: self.verbose.wrap(BoringTlsConn {
                    inner: TokioIo::new(io),
                }),
                is_proxy: false,
                tls_info: self.tls_info,
            });
        }

        dst.set_uri(proxy_dst);

        self.connect_with_maybe_proxy(dst, true).await
    }
}

async fn with_timeout<T, F>(f: F, timeout: Option<Duration>) -> Result<T, BoxError>
where
    F: Future<Output = Result<T, BoxError>>,
{
    if let Some(to) = timeout {
        match tokio::time::timeout(to, f).await {
            Err(_elapsed) => Err(Box::new(crate::error::TimedOut) as BoxError),
            Ok(Ok(try_res)) => Ok(try_res),
            Ok(Err(e)) => Err(e),
        }
    } else {
        f.await
    }
}

impl Service<Dst> for ConnectorService {
    type Response = Conn;
    type Error = BoxError;
    type Future = Connecting;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut dst: Dst) -> Self::Future {
        debug!("starting new connection: {:?}", dst.uri());

        if let Some(proxy_scheme) = dst.take_proxy_intercepted() {
            return Box::pin(with_timeout(
                self.clone().connect_via_proxy(dst, proxy_scheme),
                self.timeout,
            ));
        } else {
            for prox in self.proxies.iter() {
                if let Some(intercepted) = prox.intercept(dst.uri()) {
                    return Box::pin(with_timeout(
                        self.clone().connect_via_proxy(dst, intercepted),
                        self.timeout,
                    ));
                }
            }
        }

        Box::pin(with_timeout(
            self.clone().connect_with_maybe_proxy(dst, false),
            self.timeout,
        ))
    }
}

trait TlsInfoFactory {
    fn tls_info(&self) -> Option<crate::tls::TlsInfo>;
}

impl TlsInfoFactory for tokio::net::TcpStream {
    fn tls_info(&self) -> Option<crate::tls::TlsInfo> {
        None
    }
}

impl<T: TlsInfoFactory> TlsInfoFactory for TokioIo<T> {
    fn tls_info(&self) -> Option<crate::tls::TlsInfo> {
        self.inner().tls_info()
    }
}

impl TlsInfoFactory for SslStream<TokioIo<TokioIo<tokio::net::TcpStream>>> {
    fn tls_info(&self) -> Option<crate::tls::TlsInfo> {
        self.ssl()
            .peer_certificate()
            .and_then(|c| c.to_der().ok())
            .map(|c| crate::tls::TlsInfo {
                peer_certificate: Some(c),
            })
    }
}

impl TlsInfoFactory for SslStream<TokioIo<MaybeHttpsStream<TokioIo<tokio::net::TcpStream>>>> {
    fn tls_info(&self) -> Option<crate::tls::TlsInfo> {
        self.get_ref().inner().tls_info()
    }
}

impl TlsInfoFactory for MaybeHttpsStream<TokioIo<tokio::net::TcpStream>> {
    fn tls_info(&self) -> Option<crate::tls::TlsInfo> {
        match self {
            MaybeHttpsStream::Https(tls) => tls.inner().tls_info(),
            MaybeHttpsStream::Http(_) => None,
        }
    }
}

pub(crate) trait AsyncConn:
    Read + Write + Connection + Send + Sync + Unpin + 'static
{
}

impl<T: Read + Write + Connection + Send + Sync + Unpin + 'static> AsyncConn for T {}

trait AsyncConnWithInfo: AsyncConn + TlsInfoFactory {}

impl<T: AsyncConn + TlsInfoFactory> AsyncConnWithInfo for T {}

type BoxConn = Box<dyn AsyncConnWithInfo>;

pub(crate) mod sealed {
    use super::*;

    #[derive(Debug)]
    pub struct Unnameable(pub(super) Dst);

    pin_project! {
        /// Note: the `is_proxy` member means *is plain text HTTP proxy*.
        /// This tells hyper whether the URI should be written in
        /// * origin-form (`GET /just/a/path HTTP/1.1`), when `is_proxy == false`, or
        /// * absolute-form (`GET http://foo.bar/and/a/path HTTP/1.1`), otherwise.
        pub struct Conn {
            #[pin]
            pub(super) inner: BoxConn,
            pub(super) is_proxy: bool,
            // Only needed for __tls, but #[cfg()] on fields breaks pin_project!
            pub(super) tls_info: bool,
        }
    }

    impl Connection for Conn {
        fn connected(&self) -> Connected {
            let connected = self.inner.connected().proxy(self.is_proxy);

            if self.tls_info {
                if let Some(tls_info) = self.inner.tls_info() {
                    connected.extra(tls_info)
                } else {
                    connected
                }
            } else {
                connected
            }
        }
    }

    impl Read for Conn {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context,
            buf: ReadBufCursor<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.project();
            Read::poll_read(this.inner, cx, buf)
        }
    }

    impl Write for Conn {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            let this = self.project();
            Write::poll_write(this.inner, cx, buf)
        }

        fn poll_write_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<Result<usize, io::Error>> {
            let this = self.project();
            Write::poll_write_vectored(this.inner, cx, bufs)
        }

        fn is_write_vectored(&self) -> bool {
            self.inner.is_write_vectored()
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), io::Error>> {
            let this = self.project();
            Write::poll_flush(this.inner, cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), io::Error>> {
            let this = self.project();
            Write::poll_shutdown(this.inner, cx)
        }
    }
}

pub(crate) type Connecting = Pin<Box<dyn Future<Output = Result<Conn, BoxError>> + Send>>;

mod tls_conn {
    use super::TlsInfoFactory;
    use crate::core::rt::{Read, ReadBufCursor, Write};
    use crate::{
        core::client::connect::{Connected, Connection},
        core::rt::TokioIo,
        tls::MaybeHttpsStream,
    };
    use pin_project_lite::pin_project;
    use std::{
        io::{self, IoSlice},
        pin::Pin,
        task::{Context, Poll},
    };
    use tokio::{
        io::{AsyncRead, AsyncWrite},
        net::TcpStream,
    };
    use tokio_boring2::SslStream;

    pin_project! {
        pub(super) struct BoringTlsConn<T> {
            #[pin] pub(super) inner: TokioIo<SslStream<T>>,
        }
    }

    impl Connection for BoringTlsConn<TokioIo<TokioIo<TcpStream>>> {
        fn connected(&self) -> Connected {
            let connected = self.inner.inner().get_ref().connected();
            if self.inner.inner().ssl().selected_alpn_protocol() == Some(b"h2") {
                connected.negotiated_h2()
            } else {
                connected
            }
        }
    }

    impl Connection for BoringTlsConn<TokioIo<MaybeHttpsStream<TokioIo<TcpStream>>>> {
        fn connected(&self) -> Connected {
            let connected = self.inner.inner().get_ref().connected();
            if self.inner.inner().ssl().selected_alpn_protocol() == Some(b"h2") {
                connected.negotiated_h2()
            } else {
                connected
            }
        }
    }

    impl<T: AsyncRead + AsyncWrite + Unpin> Read for BoringTlsConn<T> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context,
            buf: ReadBufCursor<'_>,
        ) -> Poll<tokio::io::Result<()>> {
            let this = self.project();
            Read::poll_read(this.inner, cx, buf)
        }
    }

    impl<T: AsyncRead + AsyncWrite + Unpin> Write for BoringTlsConn<T> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &[u8],
        ) -> Poll<Result<usize, tokio::io::Error>> {
            let this = self.project();
            Write::poll_write(this.inner, cx, buf)
        }

        fn poll_write_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<Result<usize, io::Error>> {
            let this = self.project();
            Write::poll_write_vectored(this.inner, cx, bufs)
        }

        fn is_write_vectored(&self) -> bool {
            self.inner.is_write_vectored()
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut Context,
        ) -> Poll<Result<(), tokio::io::Error>> {
            let this = self.project();
            Write::poll_flush(this.inner, cx)
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            cx: &mut Context,
        ) -> Poll<Result<(), tokio::io::Error>> {
            let this = self.project();
            Write::poll_shutdown(this.inner, cx)
        }
    }

    impl<T> TlsInfoFactory for BoringTlsConn<T>
    where
        TokioIo<SslStream<T>>: TlsInfoFactory,
    {
        fn tls_info(&self) -> Option<crate::tls::TlsInfo> {
            self.inner.tls_info()
        }
    }
}

#[cfg(feature = "socks")]
mod socks {
    use std::{io, net::SocketAddr};

    use http::Uri;
    use tokio::net::TcpStream;
    use tokio_socks::{
        IntoTargetAddr,
        tcp::{Socks4Stream, Socks5Stream},
    };

    use super::{BoxError, Scheme};
    use crate::{dns::DynResolver, proxy::Intercepted};

    pub(super) enum DnsResolve {
        Local,
        Proxy,
    }

    pub(super) async fn connect(
        proxy: Intercepted,
        dst: &Uri,
        dns_mode: DnsResolve,
        resolver: &DynResolver,
    ) -> Result<TcpStream, BoxError> {
        let (host, port) = extract_host_port(dst)?;

        let proxy_addr = resolve_proxy_addr(&proxy, resolver).await?;

        let target = resolve_target_addr(host, port, dst, &dns_mode, resolver).await?;

        match proxy.uri().scheme_str() {
            Some("socks4" | "socks4a") => {
                let stream = Socks4Stream::connect(proxy_addr, target)
                    .await
                    .map_err(|e| format!("SOCKS4 connect error: {e}"))?;

                Ok(stream.into_inner())
            }
            Some("socks5" | "socks5h") => match proxy.raw_auth() {
                Some((user, pass)) => {
                    let stream =
                        Socks5Stream::connect_with_password(proxy_addr, target, user, pass)
                            .await
                            .map_err(|e| format!("SOCKS5 connect error: {e}"))?;

                    Ok(stream.into_inner())
                }
                None => {
                    let stream = Socks5Stream::connect(proxy_addr, target)
                        .await
                        .map_err(|e| format!("SOCKS5 connect error: {e}"))?;

                    Ok(stream.into_inner())
                }
            },
            _ => unreachable!("connect is only called for socks proxies"),
        }
    }

    fn extract_host_port(dst: &Uri) -> Result<(&str, u16), BoxError> {
        let https = dst.scheme() == Some(&Scheme::HTTPS);
        let host = dst
            .host()
            .ok_or_else(|| io::Error::other("no host in URI"))?;
        let port = dst
            .port()
            .map(|p| p.as_u16())
            .unwrap_or(if https { 443 } else { 80 });
        Ok((host, port))
    }

    async fn resolve_proxy_addr(
        proxy: &Intercepted,
        resolver: &DynResolver,
    ) -> Result<SocketAddr, BoxError> {
        resolver
            .http_resolve(proxy.uri())
            .await?
            .next()
            .ok_or_else(|| "proxy DNS resolve returned empty".into())
    }

    async fn resolve_target_addr<'a>(
        host: &'a str,
        port: u16,
        dst: &Uri,
        dns_mode: &DnsResolve,
        resolver: &DynResolver,
    ) -> Result<tokio_socks::TargetAddr<'a>, BoxError> {
        match dns_mode {
            DnsResolve::Local => {
                if let Some(addr) = resolver.http_resolve(dst).await?.next() {
                    Ok(addr.into_target_addr()?)
                } else {
                    Ok((host, port).into_target_addr()?)
                }
            }
            DnsResolve::Proxy => Ok((host, port).into_target_addr()?),
        }
    }
}

mod verbose {
    use crate::core::client::connect::{Connected, Connection};
    use crate::core::rt::{Read, ReadBufCursor, Write};
    use std::cmp::min;
    use std::fmt;
    use std::io::{self, IoSlice};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub(super) const OFF: Wrapper = Wrapper(false);

    #[derive(Clone, Copy)]
    pub(super) struct Wrapper(pub(super) bool);

    impl Wrapper {
        pub(super) fn wrap<T: super::AsyncConnWithInfo>(&self, conn: T) -> super::BoxConn {
            if self.0 && cfg!(feature = "tracing") {
                Box::new(Verbose {
                    // truncate is fine
                    id: crate::util::fast_random() as u32,
                    inner: conn,
                })
            } else {
                Box::new(conn)
            }
        }
    }

    struct Verbose<T> {
        #[allow(dead_code)]
        id: u32,
        inner: T,
    }

    impl<T: Connection + Read + Write + Unpin> Connection for Verbose<T> {
        fn connected(&self) -> Connected {
            self.inner.connected()
        }
    }

    impl<T: Read + Write + Unpin> Read for Verbose<T> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
            mut buf: ReadBufCursor<'_>,
        ) -> Poll<std::io::Result<()>> {
            // TODO: This _does_ forget the `init` len, so it could result in
            // re-initializing twice. Needs upstream support, perhaps.
            // SAFETY: Passing to a ReadBuf will never de-initialize any bytes.
            let mut vbuf = crate::core::rt::ReadBuf::uninit(unsafe { buf.as_mut() });
            match Pin::new(&mut self.inner).poll_read(cx, vbuf.unfilled()) {
                Poll::Ready(Ok(())) => {
                    trace!("{:08x} read: {:?}", self.id, Escape(vbuf.filled()));
                    let len = vbuf.filled().len();
                    // SAFETY: The two cursors were for the same buffer. What was
                    // filled in one is safe in the other.
                    unsafe {
                        buf.advance(len);
                    }
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl<T: Read + Write + Unpin> Write for Verbose<T> {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            match Pin::new(&mut self.inner).poll_write(cx, buf) {
                Poll::Ready(Ok(n)) => {
                    trace!("{:08x} write: {:?}", self.id, Escape(&buf[..n]));
                    Poll::Ready(Ok(n))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        }

        fn poll_write_vectored(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<Result<usize, io::Error>> {
            match Pin::new(&mut self.inner).poll_write_vectored(cx, bufs) {
                Poll::Ready(Ok(nwritten)) => {
                    trace!(
                        "{:08x} write (vectored): {:?}",
                        self.id,
                        Vectored { bufs, nwritten }
                    );
                    Poll::Ready(Ok(nwritten))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        }

        fn is_write_vectored(&self) -> bool {
            self.inner.is_write_vectored()
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
        ) -> Poll<Result<(), std::io::Error>> {
            Pin::new(&mut self.inner).poll_flush(cx)
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
        ) -> Poll<Result<(), std::io::Error>> {
            Pin::new(&mut self.inner).poll_shutdown(cx)
        }
    }

    impl<T: super::TlsInfoFactory> super::TlsInfoFactory for Verbose<T> {
        fn tls_info(&self) -> Option<crate::tls::TlsInfo> {
            self.inner.tls_info()
        }
    }

    #[allow(dead_code)]
    struct Escape<'a>(&'a [u8]);

    impl fmt::Debug for Escape<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "b\"")?;
            for &c in self.0 {
                // https://doc.rust-lang.org/reference.html#byte-escapes
                if c == b'\n' {
                    write!(f, "\\n")?;
                } else if c == b'\r' {
                    write!(f, "\\r")?;
                } else if c == b'\t' {
                    write!(f, "\\t")?;
                } else if c == b'\\' || c == b'"' {
                    write!(f, "\\{}", c as char)?;
                } else if c == b'\0' {
                    write!(f, "\\0")?;
                    // ASCII printable
                } else if (0x20..0x7f).contains(&c) {
                    write!(f, "{}", c as char)?;
                } else {
                    write!(f, "\\x{:02x}", c)?;
                }
            }
            write!(f, "\"")?;
            Ok(())
        }
    }

    #[allow(dead_code)]
    struct Vectored<'a, 'b> {
        bufs: &'a [IoSlice<'b>],
        nwritten: usize,
    }

    impl fmt::Debug for Vectored<'_, '_> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let mut left = self.nwritten;
            for buf in self.bufs.iter() {
                if left == 0 {
                    break;
                }
                let n = min(left, buf.len());
                Escape(&buf[..n]).fmt(f)?;
                left -= n;
            }
            Ok(())
        }
    }
}
