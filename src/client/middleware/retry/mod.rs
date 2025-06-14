use futures_util::future;
use http::{Request, Response};
use tower::retry::Policy;

use crate::{
    Body, client::middleware::timeout::TimeoutBody, core::body::Incoming, error::BoxError,
};

#[derive(Clone)]
pub struct Http2RetryPolicy(usize);

impl Http2RetryPolicy {
    /// Create a new `Http2RetryPolicy` policy with the specified number of attempts.
    pub const fn new(attempts: usize) -> Self {
        Self(attempts)
    }

    fn is_retryable_error(&self, err: &(dyn std::error::Error + 'static)) -> bool {
        // pop the legacy::Error
        let err = if let Some(err) = err.source() {
            err
        } else {
            return false;
        };

        if let Some(cause) = err.source() {
            if let Some(err) = cause.downcast_ref::<http2::Error>() {
                // They sent us a graceful shutdown, try with a new connection!
                if err.is_go_away()
                    && err.is_remote()
                    && err.reason() == Some(http2::Reason::NO_ERROR)
                {
                    return true;
                }

                // REFUSED_STREAM was sent from the server, which is safe to retry.
                // https://www.rfc-editor.org/rfc/rfc9113.html#section-8.7-3.2
                if err.is_reset()
                    && err.is_remote()
                    && err.reason() == Some(http2::Reason::REFUSED_STREAM)
                {
                    return true;
                }
            }
        }
        false
    }
}

type Req = Request<Body>;
type Res = Response<TimeoutBody<Incoming>>;

impl Policy<Req, Res, BoxError> for Http2RetryPolicy {
    type Future = future::Ready<()>;

    fn retry(
        &mut self,
        _req: &mut Req,
        result: &mut Result<Res, BoxError>,
    ) -> Option<Self::Future> {
        if let Err(err) = result {
            if let Some(source) = err.source() {
                if self.is_retryable_error(source) {
                    return Some(future::ready(()));
                }
            }

            // Treat all errors as failures...
            // But we limit the number of attempts...
            return if self.0 > 0 {
                trace!("Retrying HTTP/2 request, attempts left: {}", self.0);
                // Try again!
                self.0 -= 1;
                Some(future::ready(()))
            } else {
                // Used all our attempts, no retry...
                None
            };
        }

        None
    }

    fn clone_request(&mut self, req: &Req) -> Option<Req> {
        let mut new_req = Request::builder()
            .method(req.method().clone())
            .uri(req.uri().clone())
            .version(req.version())
            .body(req.body().try_clone()?)
            .ok()?;

        *new_req.headers_mut() = req.headers().clone();
        *new_req.extensions_mut() = req.extensions().clone();

        Some(new_req)
    }
}
