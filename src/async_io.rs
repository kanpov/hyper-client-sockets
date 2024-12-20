#[cfg(any(feature = "unix", feature = "firecracker"))]
use std::path::Path;

use async_io::Async;
#[cfg(any(feature = "unix", feature = "firecracker"))]
use smol_hyper::rt::FuturesIo;

use crate::Backend;

#[derive(Debug, Clone)]
pub struct AsyncIoBackend;

impl Backend for AsyncIoBackend {
    type UnixIo = FuturesIo<Async<std::os::unix::net::UnixStream>>;

    type VsockIo = FuturesIo<Async<std::fs::File>>;

    type FirecrackerIo = FuturesIo<Async<std::os::unix::net::UnixStream>>;

    async fn connect_to_unix_socket(socket_path: &Path) -> Result<Self::UnixIo, std::io::Error> {
        Ok(FuturesIo::new(
            Async::<std::os::unix::net::UnixStream>::connect(socket_path).await?,
        ))
    }

    fn connect_to_vsock_socket(
        addr: vsock::VsockAddr,
    ) -> impl std::future::Future<Output = Result<Self::VsockIo, std::io::Error>> + Send {
        todo!()
    }

    fn connect_to_firecracker_socket(
        host_socket_path: &Path,
        guest_port: u32,
    ) -> impl std::future::Future<Output = Result<Self::FirecrackerIo, std::io::Error>> + Send {
        todo!()
    }
}
