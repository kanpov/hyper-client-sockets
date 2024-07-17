use std::error::Error;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::Poll;

use hex::FromHex;
use hyper::Uri as HyperUri;
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use pin_project_lite::pin_project;
use tokio::io::{self, AsyncWrite};
use tokio::net::UnixStream;
use tower_service::Service;

/// A URI that points at a Unix socket and at the URL inside the socket
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnixSocketUri {
    hyper_uri: HyperUri,
}

impl UnixSocketUri {
    /// Create a new Unix socket URI from a given socket and in-socket URL
    pub fn new(
        socket_path: impl AsRef<Path>,
        url: impl AsRef<str>,
    ) -> Result<UnixSocketUri, Box<dyn Error>> {
        let host = hex::encode(socket_path.as_ref().to_string_lossy().to_string());
        let uri_str = format!("unix://{host}/{}", url.as_ref().trim_start_matches('/'));
        let hyper_uri = uri_str.parse::<HyperUri>().map_err(|err| Box::new(err))?;
        Ok(UnixSocketUri { hyper_uri })
    }

    fn decode(hyper_uri: &HyperUri) -> Result<PathBuf, std::io::Error> {
        if hyper_uri.scheme_str() != Some("unix") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "URI scheme on a Unix socket must be unix://",
            ));
        }

        match hyper_uri.host() {
            Some(host) => {
                let bytes = Vec::from_hex(host).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "URI host must be hex")
                })?;
                Ok(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "URI host must be present",
            )),
        }
    }
}

impl From<UnixSocketUri> for HyperUri {
    fn from(value: UnixSocketUri) -> Self {
        value.hyper_uri
    }
}

pin_project! {
    /// A hyper I/O-compatible wrapper for tokio-net's UnixStream
    #[derive(Debug)]
    pub struct UnixSocketStream {
        #[pin]
        stream: UnixStream
    }
}

impl UnixSocketStream {
    async fn connect(socket_path: PathBuf) -> Result<UnixSocketStream, io::Error> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(UnixSocketStream { stream })
    }
}

impl hyper::rt::Read for UnixSocketStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let mut tokio_io = TokioIo::new(self.project().stream);
        Pin::new(&mut tokio_io).poll_read(cx, buf)
    }
}

impl hyper::rt::Write for UnixSocketStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        self.project().stream.poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.project().stream.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.project().stream.poll_shutdown(cx)
    }
}

/// A hyper connector for a Unix socket
#[derive(Debug, Clone, Copy, Default)]
pub struct UnixSocketConnector;

impl Unpin for UnixSocketConnector {}

impl Connection for UnixSocketStream {
    fn connected(&self) -> hyper_util::client::legacy::connect::Connected {
        Connected::new()
    }
}

impl Service<HyperUri> for UnixSocketConnector {
    type Response = UnixSocketStream;

    type Error = io::Error;

    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HyperUri) -> Self::Future {
        Box::pin(async move {
            let socket_path = UnixSocketUri::decode(&req)?;
            UnixSocketStream::connect(socket_path).await
        })
    }
}
