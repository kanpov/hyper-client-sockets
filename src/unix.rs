use std::error::Error;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::Poll;

use hex::FromHex;
use hyper::Uri;
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use pin_project_lite::pin_project;
use tokio::io::{self, AsyncWrite};
use tokio::net::UnixStream;
use tower_service::Service;

use crate::io_input_err;

/// An extension trait for hyper URI allowing work with Unix URIs
pub trait UnixUriExt {
    /// Create a new Unix URI with the given socket path and in-socket URL
    fn unix(socket_path: impl AsRef<Path>, url: impl AsRef<str>) -> Result<Uri, Box<dyn Error>>;

    /// Try to deconstruct this Unix URI's socket path
    fn parse_unix(&self) -> Result<PathBuf, std::io::Error>;
}

impl UnixUriExt for Uri {
    fn unix(socket_path: impl AsRef<Path>, url: impl AsRef<str>) -> Result<Uri, Box<dyn Error>> {
        let host = hex::encode(socket_path.as_ref().to_string_lossy().to_string());
        let uri_str = format!("unix://{host}/{}", url.as_ref().trim_start_matches('/'));
        let uri = uri_str.parse::<Uri>().map_err(|err| Box::new(err))?;
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

pin_project! {
    /// A hyper I/O-compatible wrapper for tokio-net's UnixStream
    #[derive(Debug)]
    pub struct HyperUnixStream {
        #[pin]
        stream: UnixStream
    }
}

impl HyperUnixStream {
    /// Manually create the stream by connecting to the given socket path, this is useful when you're
    /// not using hyper-util's high-level Client, but the low-level hyper primitives
    pub async fn connect(socket_path: impl AsRef<Path>) -> Result<HyperUnixStream, io::Error> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(HyperUnixStream { stream })
    }
}

impl hyper::rt::Read for HyperUnixStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let mut tokio_io = TokioIo::new(self.project().stream);
        Pin::new(&mut tokio_io).poll_read(cx, buf)
    }
}

impl hyper::rt::Write for HyperUnixStream {
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
pub struct HyperUnixConnector;

impl Unpin for HyperUnixConnector {}

impl Connection for HyperUnixStream {
    fn connected(&self) -> hyper_util::client::legacy::connect::Connected {
        Connected::new()
    }
}

impl Service<Uri> for HyperUnixConnector {
    type Response = HyperUnixStream;

    type Error = io::Error;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Uri) -> Self::Future {
        Box::pin(async move {
            let socket_path = req.parse_unix()?;
            HyperUnixStream::connect(socket_path).await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hyper::Uri;

    use crate::UnixUriExt;

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
