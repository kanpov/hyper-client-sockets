use std::{
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

use hex::FromHex;
use http::Uri;
use pin_project::pin_project;

use crate::{io_input_err, Backend};

/// An extension trait for a URI that allows constructing a Unix socket URI.
pub trait UnixUriExt {
    /// Create a new Unix URI with the given socket path and in-socket URI.
    fn unix(socket_path: impl AsRef<Path>, url: impl AsRef<str>) -> Result<Uri, http::uri::InvalidUri>;

    /// Try to deconstruct this Unix URI's socket path.
    fn parse_unix(&self) -> Result<PathBuf, std::io::Error>;
}

impl UnixUriExt for Uri {
    fn unix(socket_path: impl AsRef<Path>, url: impl AsRef<str>) -> Result<Uri, http::uri::InvalidUri> {
        let host = hex::encode(socket_path.as_ref().to_string_lossy().to_string());
        let uri_str = format!("unix://{host}/{}", url.as_ref().trim_start_matches('/'));
        let uri = uri_str.parse::<Uri>()?;
        Ok(uri)
    }

    fn parse_unix(&self) -> Result<PathBuf, std::io::Error> {
        if self.scheme_str() != Some("unix") {
            return Err(io_input_err("URI scheme on a Unix socket must be unix://"));
        }

        match self.host() {
            Some(host) => {
                let bytes = Vec::from_hex(host).map_err(|_| io_input_err("URI host must be hex"))?;
                Ok(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
            }
            None => Err(io_input_err("URI host must be present")),
        }
    }
}

/// A hyper-compatible Unix socket connection.
#[pin_project]
#[derive(Debug)]
pub struct HyperUnixStream {
    #[pin]
    inner: UnixStreamInner,
}

impl HyperUnixStream {
    /// Connect to the given socket path using the given [Backend].
    pub async fn connect(socket_path: impl AsRef<Path>, backend: Backend) -> Result<Self, std::io::Error> {
        match backend {
            #[cfg(feature = "tokio-backend")]
            Backend::Tokio => Ok(Self {
                inner: UnixStreamInner::Tokio {
                    stream: hyper_util::rt::TokioIo::new(tokio::net::UnixStream::connect(socket_path.as_ref()).await?),
                },
            }),
            #[cfg(feature = "async-io-backend")]
            Backend::AsyncIo => {
                let async_stream =
                    async_io::Async::<std::os::unix::net::UnixStream>::connect(socket_path.as_ref()).await?;
                Ok(Self {
                    inner: UnixStreamInner::AsyncIo {
                        stream: smol_hyper::rt::FuturesIo::new(async_stream),
                    },
                })
            }
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }
}

#[pin_project(project = UnixStreamProj)]
#[derive(Debug)]
enum UnixStreamInner {
    #[cfg(feature = "tokio-backend")]
    Tokio {
        #[pin]
        stream: hyper_util::rt::TokioIo<tokio::net::UnixStream>,
    },
    #[cfg(feature = "async-io-backend")]
    AsyncIo {
        #[pin]
        stream: smol_hyper::rt::FuturesIo<async_io::Async<std::os::unix::net::UnixStream>>,
    },
    #[cfg(all(not(feature = "tokio-backend"), not(feature = "async-io-backend")))]
    None(()),
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
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
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
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            UnixStreamProj::Tokio { stream } => stream.poll_flush(cx),
            #[cfg(feature = "async-io-backend")]
            UnixStreamProj::AsyncIo { stream } => stream.poll_flush(cx),
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            UnixStreamProj::Tokio { stream } => stream.poll_shutdown(cx),
            #[cfg(feature = "async-io-backend")]
            UnixStreamProj::AsyncIo { stream } => stream.poll_shutdown(cx),
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "connector")))]
#[cfg(feature = "connector")]
pub mod connector {
    use std::{future::Future, pin::Pin, task::Poll};

    use http::Uri;
    use hyper_util::client::legacy::connect::{Connected, Connection};
    use tower_service::Service;

    use crate::Backend;

    use super::{HyperUnixStream, UnixUriExt};

    /// A hyper-util connector for Unix sockets.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct HyperUnixConnector {
        /// The [Backend] to use when performing connections.
        pub backend: Backend,
    }

    impl Unpin for HyperUnixConnector {}

    impl Connection for HyperUnixStream {
        fn connected(&self) -> hyper_util::client::legacy::connect::Connected {
            Connected::new()
        }
    }

    impl Service<Uri> for HyperUnixConnector {
        type Response = HyperUnixStream;

        type Error = std::io::Error;

        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Uri) -> Self::Future {
            let backend = self.backend;

            Box::pin(async move {
                let socket_path = req.parse_unix()?;
                HyperUnixStream::connect(socket_path, backend).await
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hyper::Uri;

    use crate::unix::UnixUriExt;

    #[test]
    fn unix_uri_should_be_constructed_correctly() {
        let uri_str = format!("unix://{}/route", hex::encode("/tmp/socket.sock"));
        assert_eq!(
            Uri::unix("/tmp/socket.sock", "/route").unwrap(),
            uri_str.parse::<Uri>().unwrap()
        );
    }

    #[test]
    fn unix_uri_should_be_deconstructed_correctly() {
        let uri = format!("unix://{}/route", hex::encode("/tmp/socket.sock"));
        assert_eq!(
            uri.parse::<Uri>().unwrap().parse_unix().unwrap(),
            PathBuf::from("/tmp/socket.sock")
        );
    }
}
