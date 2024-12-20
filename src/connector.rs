use std::{
    pin::Pin,
    task::{Context, Poll},
};

use hyper_util::client::legacy::connect::{Connected, Connection};

/// This is an internal wrapper over an IO type that implements [hyper::rt::Write] and
/// [hyper::rt::Read] that also implements [Connection] to achieve compatibility with hyper-util.
pub struct ConnectableIo<IO: hyper::rt::Write + hyper::rt::Read + Send + Unpin>(IO);

impl<IO: hyper::rt::Write + hyper::rt::Read + Send + Unpin> hyper::rt::Write for ConnectableIo<IO> {
    #[inline(always)]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    #[inline(always)]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    #[inline(always)]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}

impl<IO: hyper::rt::Write + hyper::rt::Read + Send + Unpin> hyper::rt::Read for ConnectableIo<IO> {
    #[inline(always)]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl<IO: hyper::rt::Write + hyper::rt::Read + Send + Unpin> Connection for ConnectableIo<IO> {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

#[cfg(feature = "unix")]
#[cfg_attr(docsrs, doc(cfg(feature = "unix")))]
pub mod unix {
    use std::{future::Future, marker::PhantomData, pin::Pin, task::Poll};

    use http::Uri;

    use crate::{uri::UnixUri, Backend};

    use super::ConnectableIo;

    #[derive(Debug, Clone)]
    pub struct UnixConnector<B: Backend> {
        marker: PhantomData<B>,
    }

    impl<B: Backend> UnixConnector<B> {
        pub fn new() -> Self {
            Self { marker: PhantomData }
        }
    }

    impl<B: Backend> tower_service::Service<Uri> for UnixConnector<B> {
        type Response = ConnectableIo<B::UnixIo>;

        type Error = std::io::Error;

        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        #[inline(always)]
        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        #[inline(always)]
        fn call(&mut self, uri: Uri) -> Self::Future {
            Box::pin(async move {
                let socket_path = uri.parse_unix()?;
                let io = B::connect_to_unix_socket(&socket_path).await?;
                Ok(ConnectableIo(io))
            })
        }
    }
}

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
pub mod vsock {
    use std::{future::Future, marker::PhantomData, pin::Pin, task::Poll};

    use http::Uri;

    use crate::{uri::VsockUri, Backend};

    use super::ConnectableIo;

    pub struct VsockConnector<B: Backend> {
        marker: PhantomData<B>,
    }

    impl<B: Backend> VsockConnector<B> {
        pub fn new() -> Self {
            Self { marker: PhantomData }
        }
    }

    impl<B: Backend> tower_service::Service<Uri> for VsockConnector<B> {
        type Response = ConnectableIo<B::VsockIo>;

        type Error = std::io::Error;

        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        #[inline(always)]
        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        #[inline(always)]
        fn call(&mut self, uri: Uri) -> Self::Future {
            Box::pin(async move {
                let addr = uri.parse_vsock()?;
                let io = B::connect_to_vsock_socket(addr).await?;
                Ok(ConnectableIo(io))
            })
        }
    }
}

#[cfg(feature = "firecracker")]
#[cfg_attr(docsrs, doc(cfg(feature = "firecracker")))]
pub mod firecracker {
    use std::{future::Future, marker::PhantomData, pin::Pin, task::Poll};

    use http::Uri;

    use crate::{uri::FirecrackerUri, Backend};

    use super::ConnectableIo;

    pub struct FirecrackerConnector<B: Backend> {
        marker: PhantomData<B>,
    }

    impl<B: Backend> FirecrackerConnector<B> {
        pub fn new() -> Self {
            Self { marker: PhantomData }
        }
    }

    impl<B: Backend> tower_service::Service<Uri> for FirecrackerConnector<B> {
        type Response = ConnectableIo<B::FirecrackerIo>;

        type Error = std::io::Error;

        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        #[inline(always)]
        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        #[inline(always)]
        fn call(&mut self, uri: Uri) -> Self::Future {
            Box::pin(async move {
                let (host_socket_path, guest_port) = uri.parse_firecracker()?;
                let io = B::connect_to_firecracker_socket(&host_socket_path, guest_port).await?;
                Ok(ConnectableIo(io))
            })
        }
    }
}
