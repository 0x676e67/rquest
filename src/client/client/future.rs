use std::{
    pin::Pin,
    task::{Context, Poll},
};

use http::{Request as HttpRequest, Response as HttpResponse};
use pin_project_lite::pin_project;
use tower::util::{BoxCloneSyncService, Oneshot};
use url::Url;

use super::{Body, Response, ResponseBody};
use crate::{
    Error,
    client::{
        body,
        middleware::{self},
    },
    core::body::Incoming,
    error::BoxError,
    into_url::IntoUrlSealed,
};

type ResponseFuture = Oneshot<
    BoxCloneSyncService<HttpRequest<Body>, HttpResponse<ResponseBody>, BoxError>,
    HttpRequest<Body>,
>;

type CoreResponseFuture = crate::core::client::ResponseFuture;

pin_project! {
    #[project = PendingProj]
    pub enum Pending {
        Request {
            url: Url,
            #[pin]
            in_flight: ResponseFuture,
        },
        Error {
            error: Option<Error>,
        },
    }
}

pin_project! {
    #[project = CorePendingProj]
    pub enum CorePending {
        Request {
            #[pin]
            fut: CoreResponseFuture,
        },
        Error {
            error: Option<Error>,
        },
    }
}

impl Pending {
    #[inline(always)]
    pub(crate) fn new(url: Url, in_flight: ResponseFuture) -> Self {
        Pending::Request { url, in_flight }
    }

    #[inline(always)]
    pub(crate) fn new_err(err: Error) -> Self {
        Pending::Error { error: Some(err) }
    }
}

impl Future for Pending {
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            PendingProj::Request { url, in_flight } => {
                let mut res = {
                    let r = in_flight.get_mut();
                    match Pin::new(r).poll(cx) {
                        Poll::Ready(Ok(res)) => res.map(body::boxed),
                        Poll::Ready(Err(e)) => {
                            let mut e = match e.downcast::<Error>() {
                                Ok(e) => *e,
                                Err(e) => Error::request(e),
                            };

                            if e.url().is_none() {
                                e = e.with_url(url.clone());
                            }

                            return Poll::Ready(Err(e));
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                };

                if let Some(uri) = res
                    .extensions_mut()
                    .remove::<middleware::redirect::RequestUri>()
                {
                    *url = IntoUrlSealed::into_url(uri.0.to_string())?;
                }

                Poll::Ready(Ok(Response::new(res, url.clone())))
            }
            PendingProj::Error { error } => Poll::Ready(Err(error
                .take()
                .expect("Error already taken in PendingInner::Error"))),
        }
    }
}

impl CorePending {
    #[inline(always)]
    pub(super) fn new(fut: CoreResponseFuture) -> Self {
        CorePending::Request { fut }
    }

    #[inline(always)]
    pub(super) fn new_err(err: Error) -> Self {
        CorePending::Error { error: Some(err) }
    }
}

impl Future for CorePending {
    type Output = Result<HttpResponse<Incoming>, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            CorePendingProj::Request { fut } => {
                let r = fut.get_mut();
                match Pin::new(r).poll(cx) {
                    Poll::Ready(Ok(res)) => Poll::Ready(Ok(res)),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                    Poll::Pending => Poll::Pending,
                }
            }
            CorePendingProj::Error { error } => Poll::Ready(Err(error
                .take()
                .unwrap_or_else(|| {
                    Error::builder("Pending future encountered an error without a specific error")
                })
                .into())),
        }
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_future_size() {
        let s = std::mem::size_of::<super::Pending>();
        assert!(s <= 360, "size_of::<Pending>() == {s}, too big");
    }

    #[tokio::test]
    async fn error_has_url() {
        let u = "http://does.not.exist.local/ever";
        let err = crate::Client::new().get(u).send().await.unwrap_err();
        assert_eq!(err.url().map(AsRef::as_ref), Some(u), "{err:?}");
    }
}
