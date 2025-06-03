use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tower::Service;

#[cfg(feature = "cookies")]
use crate::cookie;
use crate::{
    connect::Connector,
    core::{Request, body::Incoming, client::Client},
};

#[derive(Clone)]
pub struct ClientService {
    #[cfg(feature = "cookies")]
    cookie_store: Option<Arc<dyn cookie::CookieStore>>,
    client: Client<Connector, super::Body>,
}

impl ClientService {
    pub fn new(
        client: Client<Connector, super::Body>,
        #[cfg(feature = "cookies")] cookie_store: Option<Arc<dyn cookie::CookieStore + 'static>>,
    ) -> Self {
        Self {
            #[cfg(feature = "cookies")]
            cookie_store,
            client,
        }
    }
}

impl Service<Request<super::Body>> for ClientService {
    type Error = crate::Error;
    type Response = http::Response<Incoming>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.client.poll_ready(cx).map_err(crate::error::request)
    }

    #[cfg(not(feature = "cookies"))]
    fn call(&mut self, req: Request) -> Self::Future {
        let clone = self.client.clone();
        let mut inner = std::mem::replace(&mut self.hyper, clone);
        Box::pin(async move { inner.call(req).await.map_err(crate::error::request) })
    }

    #[cfg(feature = "cookies")]
    fn call(&mut self, mut req: Request<super::Body>) -> Self::Future {
        use url::Url;

        let clone = self.client.clone();
        let mut inner = std::mem::replace(&mut self.client, clone);
        let url = Url::parse(req.uri().to_string().as_str()).expect("invalid URL");

        if let Some(cookie_store) = self.cookie_store.as_ref() {
            if req.headers().get(crate::header::COOKIE).is_none() {
                let headers = req.headers_mut();
                crate::util::add_cookie_header(cookie_store, &url, headers);
            }
        }

        let cookie_store = self.cookie_store.clone();
        Box::pin(async move {
            let res = inner.call(req).await.map_err(crate::error::request);

            if let Some(ref cookie_store) = cookie_store {
                if let Ok(res) = &res {
                    let mut cookies =
                        cookie::extract_response_cookie_headers(res.headers()).peekable();
                    if cookies.peek().is_some() {
                        cookie_store.set_cookies(&mut cookies, &url);
                    }
                }
            }

            res
        })
    }
}
