use std::{
    error::Error,
    io::ErrorKind,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{io::BufReader, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, StreamExt};
use hex::FromHex;
use hyper::Uri;
use pin_project_lite::pin_project;
#[cfg(feature = "tokio-backend")]
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::{io_input_err, Backend};

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
    pub struct HyperFirecrackerStream {
        #[pin] inner: FirecrackerStreamInner
    }
}

pin_project! {
    #[project = FirecrackerStreamProj]
    enum FirecrackerStreamInner {
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
    }
}

impl HyperFirecrackerStream {
    pub async fn connect(
        host_socket_path: impl AsRef<Path>,
        guest_port: u32,
        backend: Backend,
    ) -> Result<Self, std::io::Error> {
        match backend {
            #[cfg(feature = "tokio-backend")]
            Backend::Tokio => {
                let mut stream = tokio::net::UnixStream::connect(host_socket_path.as_ref())
                    .await?
                    .compat();
                authenticate(guest_port, &mut stream).await?;

                Ok(Self {
                    inner: FirecrackerStreamInner::Tokio {
                        stream: hyper_util::rt::TokioIo::new(stream.into_inner()),
                    },
                })
            }
            #[cfg(feature = "async-io-backend")]
            Backend::AsyncIo => {
                let mut async_stream =
                    async_io::Async::<std::os::unix::net::UnixStream>::connect(host_socket_path.as_ref()).await?;
                authenticate(guest_port, &mut async_stream).await?;

                Ok(Self {
                    inner: FirecrackerStreamInner::AsyncIo {
                        stream: smol_hyper::rt::FuturesIo::new(async_stream),
                    },
                })
            }
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }
}

async fn authenticate(
    guest_port: u32,
    mut stream: impl AsyncReadExt + AsyncWriteExt + Unpin,
) -> Result<(), std::io::Error> {
    stream.write_all(format!("CONNECT {guest_port}\n").as_bytes()).await?;
    let mut buf_reader = BufReader::new(&mut stream).lines();
    match buf_reader.next().await {
        Some(Ok(line)) => {
            if !line.starts_with("OK") {
                return Err(std::io::Error::new(
                    ErrorKind::ConnectionRefused,
                    "Firecracker refused to establish a tunnel to the given guest port",
                ));
            }
        }
        _ => return Err(io_input_err("Could not read response")),
    };

    Ok(())
}

impl hyper::rt::Read for HyperFirecrackerStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            FirecrackerStreamProj::Tokio { stream } => stream.poll_read(cx, buf),
            #[cfg(feature = "async-io-backend")]
            FirecrackerStreamProj::AsyncIo { stream } => stream.poll_read(cx, buf),
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }
}

impl hyper::rt::Write for HyperFirecrackerStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            FirecrackerStreamProj::Tokio { stream } => stream.poll_write(cx, buf),
            #[cfg(feature = "async-io-backend")]
            FirecrackerStreamProj::AsyncIo { stream } => stream.poll_write(cx, buf),
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            FirecrackerStreamProj::Tokio { stream } => stream.poll_flush(cx),
            #[cfg(feature = "async-io-backend")]
            FirecrackerStreamProj::AsyncIo { stream } => stream.poll_flush(cx),
            #[allow(unreachable_patterns)]
            _ => panic!("No hyper-client-sockets backend was configured"),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project().inner.project() {
            #[cfg(feature = "tokio-backend")]
            FirecrackerStreamProj::Tokio { stream } => stream.poll_shutdown(cx),
            #[cfg(feature = "async-io-backend")]
            FirecrackerStreamProj::AsyncIo { stream } => stream.poll_shutdown(cx),
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

    use crate::Backend;

    use super::{FirecrackerUriExt, HyperFirecrackerStream};

    /// A hyper connector for a Firecracker socket
    #[derive(Debug, Clone, Copy)]
    pub struct HyperFirecrackerConnector {
        backend: Backend,
    }

    impl HyperFirecrackerConnector {
        pub fn new(backend: Backend) -> Self {
            Self { backend }
        }
    }

    impl Unpin for HyperFirecrackerConnector {}

    impl Connection for HyperFirecrackerStream {
        fn connected(&self) -> hyper_util::client::legacy::connect::Connected {
            Connected::new()
        }
    }

    impl Service<Uri> for HyperFirecrackerConnector {
        type Response = HyperFirecrackerStream;

        type Error = std::io::Error;

        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Uri) -> Self::Future {
            let backend = self.backend;

            Box::pin(async move {
                let (host_socket_path, guest_port) = req.parse_firecracker()?;
                HyperFirecrackerStream::connect(host_socket_path, guest_port, backend).await
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hyper::Uri;

    use crate::firecracker::FirecrackerUriExt;

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
