use std::{error::Error, future::Future, pin::Pin, task::Poll};

use hex::FromHex;
use hyper::Uri;
use hyper_util::{
    client::legacy::connect::{Connected, Connection},
    rt::TokioIo,
};
use pin_project_lite::pin_project;
use tower_service::Service;

use crate::io_input_err;

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
    /// A hyper I/O-compatible wrapper for tokio-vsock's VsockSteram
    #[derive(Debug)]
    pub struct HyperVsockStream {
        #[pin]
        stream: VsockStream
    }
}

impl HyperVsockStream {
    /// Manually create this stream by connecting to the given vsock CID and port, this is useful when you're
    /// not using hyper-util's high-level Client, but the low-level hyper primitives
    pub async fn connect(cid: u32, port: u32) -> Result<HyperVsockStream, std::io::Error> {
        let stream = VsockStream::connect(VsockAddr::new(cid, port)).await?;
        Ok(HyperVsockStream { stream })
    }
}

impl hyper::rt::Read for HyperVsockStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let mut tokio_io = TokioIo::new(self.project().stream);
        Pin::new(&mut tokio_io).poll_read(cx, buf)
    }
}

impl hyper::rt::Write for HyperVsockStream {
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

/// A hyper connector for a vsock
#[derive(Debug, Clone, Copy, Default)]
pub struct HyperVsockConnector;

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
        Box::pin(async move {
            let (cid, port) = req.parse_vsock()?;
            HyperVsockStream::connect(cid, port).await
        })
    }
}

#[cfg(test)]
mod tests {
    use hyper::Uri;

    use crate::VsockUriExt;

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
