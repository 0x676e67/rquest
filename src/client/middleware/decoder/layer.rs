use std::task::{Context, Poll};

use http::{Request, Response};
use http_body::Body;
use tower::Layer;
use tower_http::decompression::{
    Decompression as TowerDecompression, DecompressionBody, ResponseFuture,
};
use tower_service::Service;

use crate::{client::decoder::Accepts, config::RequestAcceptsEncoding, core::ext::RequestConfig};

/// Decompresses response bodies of the underlying service.
///
/// This adds the `Accept-Encoding` header to requests and transparently decompresses response
/// bodies based on the `Content-Encoding` header.
#[derive(Clone)]
pub struct DecompressionLayer {
    accept: Accepts,
}

impl DecompressionLayer {
    /// Creates a new `DecompressionLayer` with the specified `Accepts`.
    pub const fn new(accept: Accepts) -> Self {
        Self { accept }
    }
}

impl<S> Layer<S> for DecompressionLayer {
    type Service = Decompression<S>;

    fn layer(&self, service: S) -> Self::Service {
        Decompression::new(service, self.accept.clone())
    }
}

/// Decompresses response bodies of the underlying service.
///
/// This adds the `Accept-Encoding` header to requests and transparently decompresses response
/// bodies based on the `Content-Encoding` header.
#[derive(Clone)]
pub struct Decompression<S> {
    decoder: TowerDecompression<S>,
}

impl<S> Decompression<S> {
    /// Creates a new `Decompression` wrapping the `service`.
    pub fn new(service: S, accepts: Accepts) -> Decompression<S> {
        let decoder = TowerDecompression::new(service);
        let decoder = Self::accepts(decoder, &accepts);
        Decompression { decoder }
    }

    /// Sets decompression options based on the provided `Accepts`.
    fn accepts(mut decoder: TowerDecompression<S>, accepts: &Accepts) -> TowerDecompression<S> {
        #[cfg(feature = "gzip")]
        {
            decoder = decoder.gzip(accepts.gzip);
        }

        #[cfg(feature = "deflate")]
        {
            decoder = decoder.deflate(accepts.deflate);
        }

        #[cfg(feature = "brotli")]
        {
            decoder = decoder.br(accepts.brotli);
        }

        #[cfg(feature = "zstd")]
        {
            decoder = decoder.zstd(accepts.zstd);
        }

        decoder
    }
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for Decompression<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone,
    ReqBody: Body,
    ResBody: Body,
{
    type Response = Response<DecompressionBody<ResBody>>;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future>;

    #[inline(always)]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.decoder.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        if let Some(accpets) = RequestConfig::<RequestAcceptsEncoding>::get(req.extensions()) {
            let inner = Decompression::accepts(self.decoder.clone(), accpets);
            self.decoder = std::mem::replace(&mut self.decoder, inner);
        }

        self.decoder.call(req)
    }
}
