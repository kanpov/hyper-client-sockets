use std::{
    error::Error,
    future::Future,
    io::ErrorKind,
    path::{Path, PathBuf},
    pin::Pin,
    task::Poll,
};

use hex::FromHex;
use hyper::Uri;
use hyper_util::{
    client::legacy::connect::{Connected, Connection},
    rt::TokioIo,
};
use pin_project_lite::pin_project;
use tower_service::Service;

use crate::io_input_err;

/// An extension trait for hyper URI allowing work with Firecracker URIs
pub trait FirecrackerUriExt {
    /// Create a new Firecracker URI with the given host socket path, guest port and in-socket URL
    fn firecracker(
        host_socket_path: impl AsRef<Path>,
        guest_port: u32,
        url: impl AsRef<str>,
    ) -> Result<Uri, Box<dyn Error>>;

    /// Deconstruct this Firecracker URI into its host socket path and guest port
    fn parse_firecracker(&self) -> Result<(PathBuf, u32), std::io::Error>;
}

impl FirecrackerUriExt for Uri {
    fn firecracker(
        host_socket_path: impl AsRef<Path>,
        guest_port: u32,
        url: impl AsRef<str>,
    ) -> Result<Uri, Box<dyn Error>> {
        let host = hex::encode(format!(
            "{}:{guest_port}",
            host_socket_path.as_ref().to_string_lossy().to_string()
        ));
        let uri_str = format!("fc://{host}/{}", url.as_ref().trim_start_matches('/'));
        let uri = uri_str.parse::<Uri>().map_err(|err| Box::new(err))?;
        Ok(uri)
    }

    fn parse_firecracker(&self) -> Result<(PathBuf, u32), std::io::Error> {
        if self.scheme_str() != Some("fc") {
            return Err(io_input_err("URI scheme on a Firecracker socket must be fc://"));
        }

        let host = self.host().ok_or_else(|| io_input_err("URI host must be present"))?;
        let hex_decoded = Vec::from_hex(host).map_err(|_| io_input_err("URI host must be hex"))?;
        let full_str = String::from_utf8_lossy(&hex_decoded).into_owned();
        let splits = full_str
            .split_once(':')
            .ok_or_else(|| io_input_err("URI host could not be split in halves with a ."))?;
        let host_socket_path = PathBuf::try_from(splits.0)
            .map_err(|_| io_input_err("URI socket path could not be converted to a path"))?;
        let guest_port: u32 = splits
            .1
            .parse()
            .map_err(|_| io_input_err("URI guest port could not converted to u32"))?;

        Ok((host_socket_path, guest_port))
    }
}

pin_project! {
    /// A hyper I/O-compatible wrapper for a Firecracker socket
    #[derive(Debug)]
    pub struct HyperFirecrackerStream {
        #[pin]
        stream: UnixStream
    }
}

impl HyperFirecrackerStream {
    /// Manually create the stream by connecting to the given host socket path, requesting a tunnel to
    /// the given guest port and verifying the tunnel was established (with an OK message).
    /// This is useful when you're not using hyper-util's high-level Client, but the low-level hyper primitives.
    pub async fn connect(
        host_socket_path: impl AsRef<Path>,
        guest_port: u32,
    ) -> Result<HyperFirecrackerStream, io::Error> {
        let mut stream = UnixStream::connect(host_socket_path).await?;
        stream.write_all(format!("CONNECT {guest_port}\n").as_bytes()).await?;
        let mut buf_reader = BufReader::new(&mut stream).lines();
        let mut line = String::new();
        buf_reader.get_mut().read_line(&mut line).await?;

        if !line.starts_with("OK") {
            return Err(io::Error::new(
                ErrorKind::ConnectionRefused,
                "Firecracker refused to establish a tunnel to the given guest port",
            ));
        }

        drop(buf_reader);
        Ok(HyperFirecrackerStream { stream })
    }
}

impl hyper::rt::Read for HyperFirecrackerStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let mut tokio_io = TokioIo::new(self.project().stream);
        Pin::new(&mut tokio_io).poll_read(cx, buf)
    }
}

impl hyper::rt::Write for HyperFirecrackerStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        self.project().stream.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Result<(), std::io::Error>> {
        self.project().stream.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Result<(), std::io::Error>> {
        self.project().stream.poll_shutdown(cx)
    }
}

/// A hyper connector for a Firecracker socket
#[derive(Debug, Clone, Copy, Default)]
pub struct HyperFirecrackerConnector;

impl Unpin for HyperFirecrackerConnector {}

impl Connection for HyperFirecrackerStream {
    fn connected(&self) -> hyper_util::client::legacy::connect::Connected {
        Connected::new()
    }
}

impl Service<Uri> for HyperFirecrackerConnector {
    type Response = HyperFirecrackerStream;

    type Error = io::Error;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Uri) -> Self::Future {
        Box::pin(async move {
            let (host_socket_path, guest_port) = req.parse_firecracker()?;
            HyperFirecrackerStream::connect(host_socket_path, guest_port).await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hyper::Uri;

    use crate::FirecrackerUriExt;

    #[test]
    fn firecracker_uri_should_be_constructed_correctly() {
        let uri_str = format!("fc://{}/route", hex::encode("/tmp/socket.sock:1000"));
        assert_eq!(
            Uri::firecracker("/tmp/socket.sock", 1000, "/route").unwrap(),
            uri_str.parse::<Uri>().unwrap()
        );
    }

    #[test]
    fn firecracker_uri_should_be_deconstructed_correctly() {
        let uri = format!("fc://{}/route", hex::encode("/tmp/socket.sock:1000"))
            .parse::<Uri>()
            .unwrap();
        let (socket_path, port) = uri.parse_firecracker().unwrap();
        assert_eq!(socket_path, PathBuf::from("/tmp/socket.sock"));
        assert_eq!(port, 1000);
    }
}
