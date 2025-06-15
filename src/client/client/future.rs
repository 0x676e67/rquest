use std::{
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;
use tower::util::BoxCloneSyncService;
use url::Url;

use super::{Body, Response, ResponseBody};
use crate::{
    Error,
    client::{
        body,
        middleware::{self},
    },
    core::service::Oneshot,
    error::BoxError,
};

type ResponseFuture = Oneshot<
    BoxCloneSyncService<http::Request<Body>, http::Response<ResponseBody>, BoxError>,
    http::Request<Body>,
>;

pin_project! {
    pub struct Pending {
        #[pin]
        pub inner: PendingInner,
    }
}

enum PendingInner {
    Request(Pin<Box<PendingRequest>>),
    Error(Option<Error>),
}

pin_project! {
    pub(super) struct PendingRequest {
        pub url: Url,
        #[pin]
        pub in_flight: ResponseFuture,
    }
}

impl Pending {
    #[inline(always)]
    pub(super) fn new(request: PendingRequest) -> Pending {
        Pending {
            inner: PendingInner::Request(Box::pin(request)),
        }
    }

    #[inline(always)]
    pub(crate) fn new_err(err: Error) -> Pending {
        Pending {
            inner: PendingInner::Error(Some(err)),
        }
    }
}

impl Future for Pending {
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = self.project().inner;
        match inner.get_mut() {
            PendingInner::Request(req) => Pin::new(req).poll(cx),
            PendingInner::Error(err) => Poll::Ready(Err(err
                .take()
                .expect("Error already taken in PendingInner::Error"))),
        }
    }
}

impl Future for PendingRequest {
    type Output = Result<Response, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let res = {
            let r = self.as_mut().project().in_flight.get_mut();
            match Pin::new(r).poll(cx) {
                Poll::Ready(Err(e)) => {
                    return match e.downcast::<Error>() {
                        Ok(e) => Poll::Ready(Err(*e)),
                        Err(e) => Poll::Ready(Err(Error::request(e))),
                    };
                }
                Poll::Ready(Ok(res)) => res.map(body::boxed),
                Poll::Pending => return Poll::Pending,
            }
        };

        if let Some(uri) = res.extensions().get::<middleware::redirect::RequestUri>() {
            self.url = Url::parse(&uri.0.to_string()).map_err(Error::decode)?;
        }

        Poll::Ready(Ok(Response::new(res, self.url.clone())))
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_future_size() {
        let s = std::mem::size_of::<super::Pending>();
        assert!(s < 128, "size_of::<Pending>() == {s}, too big");
    }
}
