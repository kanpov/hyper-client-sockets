#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg_attr(docsrs, doc(cfg(feature = "unix")))]
#[cfg(feature = "unix")]
pub mod unix;

#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
#[cfg(feature = "vsock")]
pub mod vsock;

#[cfg_attr(docsrs, doc(cfg(feature = "firecracker")))]
#[cfg(feature = "firecracker")]
pub mod firecracker;

#[cfg(feature = "vsock")]
pub(crate) mod vsock_internal;

#[allow(unused)]
fn io_input_err(detail: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, detail)
}

/// A backend used by hyper-client-sockets to perform asynchronous I/O with sockets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// A backend based on Tokio.
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio-backend")))]
    #[cfg(feature = "tokio-backend")]
    Tokio,
    /// A backend based on async-io.
    #[cfg_attr(docsrs, doc(cfg(feature = "async-io-backend")))]
    #[cfg(feature = "async-io-backend")]
    AsyncIo,
}
