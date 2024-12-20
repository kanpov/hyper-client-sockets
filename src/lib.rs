#[cfg(any(feature = "unix", feature = "vsock"))]
use std::future::Future;

#[cfg(feature = "unix")]
use std::path::Path;

#[cfg(feature = "tokio-backend")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio-backend")))]
pub mod tokio;

pub trait Backend {
    #[cfg(feature = "unix")]
    type UnixIo: hyper::rt::Read + hyper::rt::Write + Send + Unpin;

    #[cfg(feature = "vsock")]
    type VsockIo: hyper::rt::Read + hyper::rt::Write + Send + Unpin;

    #[cfg(feature = "unix")]
    fn connect_to_unix_socket(socket_path: &Path) -> impl Future<Output = Result<Self::UnixIo, std::io::Error>> + Send;

    #[cfg(feature = "vsock")]
    fn connect_to_vsock_socket(
        addr: vsock::VsockAddr,
    ) -> impl Future<Output = Result<Self::VsockIo, std::io::Error>> + Send;
}
