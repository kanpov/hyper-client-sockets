use std::{
    error::Error,
    pin::Pin,
    task::{Context, Poll},
};

use hex::FromHex;
use hyper::Uri;
use pin_project_lite::pin_project;
use vsock::VsockAddr;

use crate::{io_input_err, Backend};

/// An extension trait for hyper URI allowing work with vsock URIs
pub trait VsockUriExt {
    /// Create a new vsock URI with the given vsock CID, port and in-socket URL
    fn vsock(cid: u32, port: u32, url: impl AsRef<str>) -> Result<Uri, Box<dyn Error>>;

    /// Deconstruct this vsock URI into its CID and port
    fn parse_vsock(&self) -> Result<(u32, u32), std::io::Error>;
}

impl VsockUriExt for Uri {
    fn vsock(cid: u32, port: u32, url: impl AsRef<str>) -> Result<Uri, Box<dyn Error>> {
        let host = hex::encode(format!("{cid}.{port}"));
        let uri_str = format!("vsock://{host}/{}", url.as_ref().trim_start_matches('/'));
        let uri = uri_str.parse::<Uri>().map_err(|err| Box::new(err))?;
        Ok(uri)
    }

    fn parse_vsock(&self) -> Result<(u32, u32), std::io::Error> {
        if self.scheme_str() != Some("vsock") {
            return Err(io_input_err("URI scheme on a vsock socket must be vsock://"));
        }

        match self.host() {
            Some(host) => {
                let full_str = Vec::from_hex(host)
                    .map_err(|_| io_input_err("URI host must be hex"))
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())?;
                let splits = full_str
                    .split_once('.')
                    .ok_or_else(|| io_input_err("URI host could not be split at . into 2 slices (CID, then port)"))?;
                let cid: u32 = splits
                    .0
                    .parse()
                    .map_err(|_| io_input_err("First split of URI (CID) can't be parsed"))?;
                let port: u32 = splits
                    .1
                    .parse()
                    .map_err(|_| io_input_err("Second split of URI (port) can't be parsed"))?;

                Ok((cid, port))
            }
            None => Err(io_input_err("URI host must be present")),
        }
    }
}

pin_project! {
    pub struct HyperVsockStream {
        #[pin] inner: VsockStreamInner
    }
}

impl HyperVsockStream {
    pub async fn connect(vsock_addr: VsockAddr, backend: Backend) -> Result<Self, std::io::Error> {
        match backend {
            #[cfg(feature = "tokio-backend")]
            Backend::Tokio => {
                let stream = tokio_vsock::VsockStream::connect(vsock_addr).await?;
                Ok(Self {
                    inner: VsockStreamInner::Tokio {
                        stream: hyper_util::rt::TokioIo::new(stream),
                    },
                })
            }
            #[cfg(feature = "async-io-backend")]
            Backend::AsyncIo => {
                use std::os::fd::{AsFd, AsRawFd, FromRawFd};

                let stream = vsock::VsockStream::connect(&vsock_addr)?;
                let stream = unsafe { std::fs::File::from_raw_fd(stream.as_fd().as_raw_fd()) };
                let stream = async_io::Async::new(stream)?;
                Ok(Self {
                    inner: VsockStreamInner::AsyncIo {
                        stream: smol_hyper::rt::FuturesIo::new(stream),
                    },
                })
            }
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }
}

pin_project! {
    #[project = VsockStreamProj]
    enum VsockStreamInner {
        #[cfg(feature = "tokio-backend")]
        Tokio { #[pin] stream: hyper_util::rt::TokioIo<tokio_vsock::VsockStream> },
        #[cfg(feature = "async-io-backend")]
        AsyncIo { #[pin] stream: smol_hyper::rt::FuturesIo<async_io::Async<std::fs::File>> }
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
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
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
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            VsockStreamProj::Tokio { stream } => stream.poll_flush(cx),
            #[cfg(feature = "async-io-backend")]
            VsockStreamProj::AsyncIo { stream } => stream.poll_flush(cx),
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            VsockStreamProj::Tokio { stream } => stream.poll_shutdown(cx),
            #[cfg(feature = "async-io-backend")]
            VsockStreamProj::AsyncIo { stream } => stream.poll_shutdown(cx),
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }
}

#[cfg(feature = "connector")]
pub mod connector {
    use std::{future::Future, pin::Pin, task::Poll};

    use http::Uri;
    use hyper_util::client::legacy::connect::{Connected, Connection};
    use tower_service::Service;
    use vsock::VsockAddr;

    use crate::Backend;

    use super::{HyperVsockStream, VsockUriExt};

    /// A hyper connector for a vsock
    #[derive(Debug, Clone, Copy)]
    pub struct HyperVsockConnector {
        backend: Backend,
    }

    impl HyperVsockConnector {
        pub fn new(backend: Backend) -> Self {
            Self { backend }
        }
    }

    impl Unpin for HyperVsockConnector {}

    impl Connection for HyperVsockStream {
        fn connected(&self) -> hyper_util::client::legacy::connect::Connected {
            Connected::new()
        }
    }

    impl Service<Uri> for HyperVsockConnector {
        type Response = HyperVsockStream;

        type Error = std::io::Error;

        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Uri) -> Self::Future {
            let backend = self.backend;

            Box::pin(async move {
                let (cid, port) = req.parse_vsock()?;
                HyperVsockStream::connect(VsockAddr::new(cid, port), backend).await
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use hyper::Uri;

    use crate::vsock::VsockUriExt;

    #[test]
    fn vsock_uri_should_be_constructed_correctly() {
        let uri = format!("vsock://{}/route", hex::encode("10.20"));
        assert_eq!(uri.parse::<Uri>().unwrap(), Uri::vsock(10, 20, "/route").unwrap());
    }

    #[test]
    fn vsock_uri_should_be_deconstructed_correctly() {
        let uri = format!("vsock://{}/route", hex::encode("10.20"))
            .parse::<Uri>()
            .unwrap();
        assert_eq!(uri.parse_vsock().unwrap(), (10, 20));
    }
}
