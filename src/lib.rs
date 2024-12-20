#[cfg(any(feature = "unix", feature = "vsock", feature = "firecracker"))]
use std::future::Future;

#[cfg(any(feature = "unix", feature = "firecracker"))]
use std::path::Path;

#[cfg(feature = "tokio-backend")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio-backend")))]
pub mod tokio;

#[cfg(feature = "hyper-util")]
#[cfg_attr(docsrs, doc(cfg(feature = "hyper-util")))]
pub mod uri;

#[cfg(feature = "hyper-util")]
#[cfg_attr(docsrs, doc(cfg(feature = "hyper-util")))]
pub mod connector;

/// A [Backend] is a runtime- and reactor-agnostic way to use hyper client-side with various types of sockets.
pub trait Backend {
    /// An IO object representing a connected Unix socket.
    #[cfg(feature = "unix")]
    #[cfg_attr(docsrs, doc(cfg(feature = "unix")))]
    type UnixIo: hyper::rt::Read + hyper::rt::Write + Send + Unpin;

    /// An IO object representing a connected virtio-vsock socket.
    #[cfg(feature = "vsock")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
    type VsockIo: hyper::rt::Read + hyper::rt::Write + Send + Unpin;

    /// An IO object representing a connected Firecracker socket (a specialized Unix socket).
    #[cfg(feature = "firecracker")]
    #[cfg_attr(docsrs, doc(cfg(feature = "firecracker")))]
    type FirecrackerIo: hyper::rt::Read + hyper::rt::Write + Send + Unpin;

    /// Connect to a Unix socket at the given [Path].
    #[cfg(feature = "unix")]
    #[cfg_attr(docsrs, doc(cfg(feature = "unix")))]
    fn connect_to_unix_socket(socket_path: &Path) -> impl Future<Output = Result<Self::UnixIo, std::io::Error>> + Send;

    /// Connect to a virtio-vsock socket at the given vsock address.
    #[cfg(feature = "vsock")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
    fn connect_to_vsock_socket(
        addr: vsock::VsockAddr,
    ) -> impl Future<Output = Result<Self::VsockIo, std::io::Error>> + Send;

    /// Connect to a Firecracker socket at the given [Path], establishing a tunnel to the given
    /// guest vsock port.
    #[cfg(feature = "firecracker")]
    #[cfg_attr(docsrs, doc(cfg(feature = "firecracker")))]
    fn connect_to_firecracker_socket(
        host_socket_path: &Path,
        guest_port: u32,
    ) -> impl Future<Output = Result<Self::FirecrackerIo, std::io::Error>> + Send;
}
