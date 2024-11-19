use std::{
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

#[cfg(feature = "tokio-backend")]
use hyper_util::rt::TokioIo;
use pin_project_lite::pin_project;
#[cfg(feature = "async-io-backend")]
use smol_hyper::rt::FuturesIo;
use vsock::VsockAddr;

pin_project! {
    pub struct HyperUnixStream {
        #[pin] inner: UnixStreamInner
    }
}

impl HyperUnixStream {
    #[cfg(feature = "tokio-backend")]
    pub async fn connect_tokio(socket_path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        Ok(Self {
            inner: UnixStreamInner::Tokio {
                stream: TokioIo::new(tokio::net::UnixStream::connect(socket_path.as_ref()).await?),
            },
        })
    }

    #[cfg(feature = "async-io-backend")]
    pub async fn connect_async_io(socket_path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let async_stream = async_io::Async::<std::os::unix::net::UnixStream>::connect(socket_path.as_ref()).await?;
        Ok(Self {
            inner: UnixStreamInner::AsyncIo {
                stream: FuturesIo::new(async_stream),
            },
        })
    }
}

pin_project! {
    #[project = UnixStreamProj]
    enum UnixStreamInner {
        #[cfg(feature = "tokio-backend")]
        Tokio { #[pin] stream: TokioIo<tokio::net::UnixStream> },
        #[cfg(feature = "async-io-backend")]
        AsyncIo {
            #[pin] stream: FuturesIo<async_io::Async<std::os::unix::net::UnixStream>>,
        },
    }
}

impl hyper::rt::Read for HyperUnixStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            UnixStreamProj::Tokio { stream } => stream.poll_read(cx, buf),
            #[cfg(feature = "async-io-backend")]
            UnixStreamProj::AsyncIo { stream } => stream.poll_read(cx, buf),
        }
    }
}

impl hyper::rt::Write for HyperUnixStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            UnixStreamProj::Tokio { stream } => stream.poll_write(cx, buf),
            #[cfg(feature = "async-io-backend")]
            UnixStreamProj::AsyncIo { stream } => stream.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            UnixStreamProj::Tokio { stream } => stream.poll_flush(cx),
            #[cfg(feature = "async-io-backend")]
            UnixStreamProj::AsyncIo { stream } => stream.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            UnixStreamProj::Tokio { stream } => stream.poll_shutdown(cx),
            #[cfg(feature = "async-io-backend")]
            UnixStreamProj::AsyncIo { stream } => stream.poll_shutdown(cx),
        }
    }
}

pin_project! {
    pub struct HyperVsockStream {
        #[pin] inner: VsockStreamInner
    }
}

impl HyperVsockStream {
    #[cfg(feature = "tokio-backend")]
    pub async fn connect_tokio(vsock_addr: VsockAddr) -> Result<Self, std::io::Error> {
        let stream = tokio_vsock::VsockStream::connect(vsock_addr).await?;
        Ok(Self {
            inner: VsockStreamInner::Tokio {
                stream: TokioIo::new(stream),
            },
        })
    }

    #[cfg(feature = "async-io-backend")]
    pub async fn connect_async_io(vsock_addr: VsockAddr) -> Result<Self, std::io::Error> {
        use std::os::fd::{AsFd, AsRawFd, FromRawFd};

        let stream = vsock::VsockStream::connect(&vsock_addr)?;
        let stream = unsafe { std::fs::File::from_raw_fd(stream.as_fd().as_raw_fd()) };
        let stream = async_io::Async::new(stream)?;
        Ok(Self {
            inner: VsockStreamInner::AsyncIo {
                stream: FuturesIo::new(stream),
            },
        })
    }
}

pin_project! {
    #[project = VsockStreamProj]
    enum VsockStreamInner {
        #[cfg(feature = "tokio-backend")]
        Tokio { #[pin] stream: TokioIo<tokio_vsock::VsockStream> },
        #[cfg(feature = "async-io-backend")]
        AsyncIo { #[pin] stream: FuturesIo<async_io::Async<std::fs::File>> }
    }
}

impl hyper::rt::Read for HyperVsockStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            VsockStreamProj::Tokio { stream } => stream.poll_read(cx, buf),
            #[cfg(feature = "async-io-backend")]
            VsockStreamProj::AsyncIo { stream } => stream.poll_read(cx, buf),
        }
    }
}

impl hyper::rt::Write for HyperVsockStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            VsockStreamProj::Tokio { stream } => stream.poll_write(cx, buf),
            #[cfg(feature = "async-io-backend")]
            VsockStreamProj::AsyncIo { stream } => stream.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            VsockStreamProj::Tokio { stream } => stream.poll_flush(cx),
            #[cfg(feature = "async-io-backend")]
            VsockStreamProj::AsyncIo { stream } => stream.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            VsockStreamProj::Tokio { stream } => stream.poll_shutdown(cx),
            #[cfg(feature = "async-io-backend")]
            VsockStreamProj::AsyncIo { stream } => stream.poll_shutdown(cx),
        }
    }
}
