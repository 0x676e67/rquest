//! HTTP/2 client connections

use std::error::Error;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use http::{Request, Response};

use crate::core::body::{Body, Incoming as IncomingBody};
use crate::core::client::dispatch::{self, TrySendError};
use crate::core::common::time::Time;
use crate::core::proto;
use crate::core::rt::Timer;
use crate::core::rt::bounds::Http2ClientConnExec;
use crate::core::rt::{Read, Write};
use crate::http2::Http2Config;

/// The sender side of an established connection.
pub struct SendRequest<B> {
    dispatch: dispatch::UnboundedSender<Request<B>, Response<IncomingBody>>,
}

impl<B> Clone for SendRequest<B> {
    fn clone(&self) -> SendRequest<B> {
        SendRequest {
            dispatch: self.dispatch.clone(),
        }
    }
}

/// A future that processes all HTTP state for the IO object.
///
/// In most cases, this should just be spawned into an executor, so that it
/// can process incoming and outgoing messages, notice hangups, and the like.
///
/// Instances of this type are typically created via the [`handshake`] function
#[must_use = "futures do nothing unless polled"]
pub struct Connection<T, B, E>
where
    T: Read + Write + Unpin,
    B: Body + 'static,
    E: Http2ClientConnExec<B, T> + Unpin,
    B::Error: Into<Box<dyn Error + Send + Sync>>,
{
    inner: (PhantomData<T>, proto::h2::ClientTask<B, E, T>),
}

/// A builder to configure an HTTP connection.
///
/// After setting options, the builder is used to create a handshake future.
///
/// **Note**: The default values of options are *not considered stable*. They
/// are subject to change at any time.
#[derive(Clone, Debug)]
pub struct Builder<Ex> {
    pub(super) exec: Ex,
    pub(super) timer: Time,
    h2_builder: proto::h2::client::Config,
}

// ===== impl SendRequest

impl<B> SendRequest<B> {
    /// Polls to determine whether this sender can be used yet for a request.
    ///
    /// If the associated connection is closed, this returns an Error.
    pub fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<crate::core::Result<()>> {
        if self.is_closed() {
            Poll::Ready(Err(crate::core::Error::new_closed()))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    /// Waits until the dispatcher is ready
    ///
    /// If the associated connection is closed, this returns an Error.
    pub async fn ready(&mut self) -> crate::core::Result<()> {
        std::future::poll_fn(|cx| self.poll_ready(cx)).await
    }

    /// Checks if the connection is currently ready to send a request.
    ///
    /// # Note
    ///
    /// This is mostly a hint. Due to inherent latency of networks, it is
    /// possible that even after checking this is ready, sending a request
    /// may still fail because the connection was closed in the meantime.
    pub fn is_ready(&self) -> bool {
        self.dispatch.is_ready()
    }

    /// Checks if the connection side has been closed.
    pub fn is_closed(&self) -> bool {
        self.dispatch.is_closed()
    }
}

impl<B> SendRequest<B>
where
    B: Body + 'static,
{
    /// Sends a `Request` on the associated connection.
    ///
    /// Returns a future that if successful, yields the `Response`.
    ///
    /// `req` must have a `Host` header.
    ///
    /// Absolute-form `Uri`s are not required. If received, they will be serialized
    /// as-is.
    pub fn send_request(
        &mut self,
        req: Request<B>,
    ) -> impl Future<Output = crate::core::Result<Response<IncomingBody>>> {
        let sent = self.dispatch.send(req);

        async move {
            match sent {
                Ok(rx) => match rx.await {
                    Ok(Ok(resp)) => Ok(resp),
                    Ok(Err(err)) => Err(err),
                    // this is definite bug if it happens, but it shouldn't happen!
                    Err(_canceled) => panic!("dispatch dropped without returning error"),
                },
                Err(_req) => {
                    debug!("connection was not ready");

                    Err(crate::core::Error::new_canceled().with("connection was not ready"))
                }
            }
        }
    }

    /// Sends a `Request` on the associated connection.
    ///
    /// Returns a future that if successful, yields the `Response`.
    ///
    /// # Error
    ///
    /// If there was an error before trying to serialize the request to the
    /// connection, the message will be returned as part of this error.
    pub fn try_send_request(
        &mut self,
        req: Request<B>,
    ) -> impl Future<Output = Result<Response<IncomingBody>, TrySendError<Request<B>>>> {
        let sent = self.dispatch.try_send(req);
        async move {
            match sent {
                Ok(rx) => match rx.await {
                    Ok(Ok(res)) => Ok(res),
                    Ok(Err(err)) => Err(err),
                    // this is definite bug if it happens, but it shouldn't happen!
                    Err(_) => panic!("dispatch dropped without returning error"),
                },
                Err(req) => {
                    debug!("connection was not ready");
                    let error = crate::core::Error::new_canceled().with("connection was not ready");
                    Err(TrySendError {
                        error,
                        message: Some(req),
                    })
                }
            }
        }
    }
}

impl<B> fmt::Debug for SendRequest<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SendRequest").finish()
    }
}

// ===== impl Connection

impl<T, B, E> Connection<T, B, E>
where
    T: Read + Write + Unpin + 'static,
    B: Body + Unpin + 'static,
    B::Data: Send,
    B::Error: Into<Box<dyn Error + Send + Sync>>,
    E: Http2ClientConnExec<B, T> + Unpin,
{
    /// Returns whether the [extended CONNECT protocol][1] is enabled or not.
    ///
    /// This setting is configured by the server peer by sending the
    /// [`SETTINGS_ENABLE_CONNECT_PROTOCOL` parameter][2] in a `SETTINGS` frame.
    /// This method returns the currently acknowledged value received from the
    /// remote.
    ///
    /// [1]: https://datatracker.ietf.org/doc/html/rfc8441#section-4
    /// [2]: https://datatracker.ietf.org/doc/html/rfc8441#section-3
    pub fn is_extended_connect_protocol_enabled(&self) -> bool {
        self.inner.1.is_extended_connect_protocol_enabled()
    }
}

impl<T, B, E> fmt::Debug for Connection<T, B, E>
where
    T: Read + Write + fmt::Debug + 'static + Unpin,
    B: Body + 'static,
    E: Http2ClientConnExec<B, T> + Unpin,
    B::Error: Into<Box<dyn Error + Send + Sync>>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection").finish()
    }
}

impl<T, B, E> Future for Connection<T, B, E>
where
    T: Read + Write + Unpin + 'static,
    B: Body + 'static + Unpin,
    B::Data: Send,
    E: Unpin,
    B::Error: Into<Box<dyn Error + Send + Sync>>,
    E: Http2ClientConnExec<B, T> + Unpin,
{
    type Output = crate::core::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match ready!(Pin::new(&mut self.inner.1).poll(cx))? {
            proto::Dispatched::Shutdown => Poll::Ready(Ok(())),
            proto::Dispatched::Upgrade(_pending) => unreachable!("http2 cannot upgrade"),
        }
    }
}

// ===== impl Builder

impl<Ex> Builder<Ex>
where
    Ex: Clone,
{
    /// Creates a new connection builder.
    #[inline]
    pub fn new(exec: Ex) -> Builder<Ex> {
        Builder {
            exec,
            timer: Time::Empty,
            h2_builder: Default::default(),
        }
    }

    /// Provide a timer to execute background HTTP2 tasks.
    pub fn timer<M>(&mut self, timer: M) -> &mut Builder<Ex>
    where
        M: Timer + Send + Sync + 'static,
    {
        self.timer = Time::Timer(Arc::new(timer));
        self
    }

    /// Provide a configuration for HTTP/2.
    pub fn config(&mut self, config: Http2Config) -> &mut Builder<Ex> {
        self.h2_builder = config.h2_builder;
        self
    }

    /// Constructs a connection with the configured options and IO.
    /// See [`client::conn`](crate::client::conn) for more.
    ///
    /// Note, if [`Connection`] is not `await`-ed, [`SendRequest`] will
    /// do nothing.
    pub fn handshake<T, B>(
        &self,
        io: T,
    ) -> impl Future<Output = crate::core::Result<(SendRequest<B>, Connection<T, B, Ex>)>>
    where
        T: Read + Write + Unpin,
        B: Body + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn Error + Send + Sync>>,
        Ex: Http2ClientConnExec<B, T> + Unpin,
    {
        let opts = self.clone();

        async move {
            trace!("client handshake HTTP/2");

            let (tx, rx) = dispatch::channel();
            let h2 = proto::h2::client::handshake(io, rx, &opts.h2_builder, opts.exec, opts.timer)
                .await?;
            Ok((
                SendRequest {
                    dispatch: tx.unbound(),
                },
                Connection {
                    inner: (PhantomData, h2),
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Builder, Connection, SendRequest};
    use crate::core::body::Body;
    use crate::core::rt::{Read, Write, bounds::Http2ClientConnExec};

    pub async fn handshake<E, T, B>(
        exec: E,
        io: T,
    ) -> crate::core::Result<(SendRequest<B>, Connection<T, B, E>)>
    where
        T: Read + Write + Unpin,
        B: Body + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        E: Http2ClientConnExec<B, T> + Unpin + Clone,
    {
        Builder::new(exec).handshake(io).await
    }

    #[tokio::test]
    #[ignore] // only compilation is checked
    async fn send_sync_executor_of_non_send_futures() {
        #[derive(Clone)]
        struct LocalTokioExecutor;

        impl<F> crate::core::rt::Executor<F> for LocalTokioExecutor
        where
            F: std::future::Future + 'static, // not requiring `Send`
        {
            fn execute(&self, fut: F) {
                // This will spawn into the currently running `LocalSet`.
                tokio::task::spawn_local(fut);
            }
        }

        #[allow(unused)]
        async fn run(io: impl crate::core::rt::Read + crate::core::rt::Write + Unpin + 'static) {
            let (_sender, conn) =
                handshake::<_, _, http_body_util::Empty<bytes::Bytes>>(LocalTokioExecutor, io)
                    .await
                    .unwrap();

            tokio::task::spawn_local(async move {
                conn.await.unwrap();
            });
        }
    }

    #[tokio::test]
    #[ignore] // only compilation is checked
    async fn not_send_not_sync_executor_of_not_send_futures() {
        #[derive(Clone)]
        struct LocalTokioExecutor {
            _x: std::marker::PhantomData<std::rc::Rc<()>>,
        }

        impl<F> crate::core::rt::Executor<F> for LocalTokioExecutor
        where
            F: std::future::Future + 'static, // not requiring `Send`
        {
            fn execute(&self, fut: F) {
                // This will spawn into the currently running `LocalSet`.
                tokio::task::spawn_local(fut);
            }
        }

        #[allow(unused)]
        async fn run(io: impl crate::core::rt::Read + crate::core::rt::Write + Unpin + 'static) {
            let (_sender, conn) = handshake::<_, _, http_body_util::Empty<bytes::Bytes>>(
                LocalTokioExecutor {
                    _x: Default::default(),
                },
                io,
            )
            .await
            .unwrap();

            tokio::task::spawn_local(async move {
                conn.await.unwrap();
            });
        }
    }

    #[tokio::test]
    #[ignore] // only compilation is checked
    async fn send_not_sync_executor_of_not_send_futures() {
        #[derive(Clone)]
        struct LocalTokioExecutor {
            _x: std::marker::PhantomData<std::cell::Cell<()>>,
        }

        impl<F> crate::core::rt::Executor<F> for LocalTokioExecutor
        where
            F: std::future::Future + 'static, // not requiring `Send`
        {
            fn execute(&self, fut: F) {
                // This will spawn into the currently running `LocalSet`.
                tokio::task::spawn_local(fut);
            }
        }

        #[allow(unused)]
        async fn run(io: impl crate::core::rt::Read + crate::core::rt::Write + Unpin + 'static) {
            let (_sender, conn) = handshake::<_, _, http_body_util::Empty<bytes::Bytes>>(
                LocalTokioExecutor {
                    _x: Default::default(),
                },
                io,
            )
            .await
            .unwrap();

            tokio::task::spawn_local(async move {
                conn.await.unwrap();
            });
        }
    }

    #[tokio::test]
    #[ignore] // only compilation is checked
    async fn send_sync_executor_of_send_futures() {
        #[derive(Clone)]
        struct TokioExecutor;

        impl<F> crate::core::rt::Executor<F> for TokioExecutor
        where
            F: std::future::Future + 'static + Send,
            F::Output: Send + 'static,
        {
            fn execute(&self, fut: F) {
                tokio::task::spawn(fut);
            }
        }

        #[allow(unused)]
        async fn run(
            io: impl crate::core::rt::Read + crate::core::rt::Write + Send + Unpin + 'static,
        ) {
            let (_sender, conn) =
                handshake::<_, _, http_body_util::Empty<bytes::Bytes>>(TokioExecutor, io)
                    .await
                    .unwrap();

            tokio::task::spawn(async move {
                conn.await.unwrap();
            });
        }
    }

    #[tokio::test]
    #[ignore] // only compilation is checked
    async fn not_send_not_sync_executor_of_send_futures() {
        #[derive(Clone)]
        struct TokioExecutor {
            // !Send, !Sync
            _x: std::marker::PhantomData<std::rc::Rc<()>>,
        }

        impl<F> crate::core::rt::Executor<F> for TokioExecutor
        where
            F: std::future::Future + 'static + Send,
            F::Output: Send + 'static,
        {
            fn execute(&self, fut: F) {
                tokio::task::spawn(fut);
            }
        }

        #[allow(unused)]
        async fn run(
            io: impl crate::core::rt::Read + crate::core::rt::Write + Send + Unpin + 'static,
        ) {
            let (_sender, conn) = handshake::<_, _, http_body_util::Empty<bytes::Bytes>>(
                TokioExecutor {
                    _x: Default::default(),
                },
                io,
            )
            .await
            .unwrap();

            tokio::task::spawn_local(async move {
                // can't use spawn here because when executor is !Send
                conn.await.unwrap();
            });
        }
    }

    #[tokio::test]
    #[ignore] // only compilation is checked
    async fn send_not_sync_executor_of_send_futures() {
        #[derive(Clone)]
        struct TokioExecutor {
            // !Sync
            _x: std::marker::PhantomData<std::cell::Cell<()>>,
        }

        impl<F> crate::core::rt::Executor<F> for TokioExecutor
        where
            F: std::future::Future + 'static + Send,
            F::Output: Send + 'static,
        {
            fn execute(&self, fut: F) {
                tokio::task::spawn(fut);
            }
        }

        #[allow(unused)]
        async fn run(
            io: impl crate::core::rt::Read + crate::core::rt::Write + Send + Unpin + 'static,
        ) {
            let (_sender, conn) = handshake::<_, _, http_body_util::Empty<bytes::Bytes>>(
                TokioExecutor {
                    _x: Default::default(),
                },
                io,
            )
            .await
            .unwrap();

            tokio::task::spawn_local(async move {
                // can't use spawn here because when executor is !Send
                conn.await.unwrap();
            });
        }
    }
}
