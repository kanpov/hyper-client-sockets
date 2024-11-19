#[cfg(feature = "unix")]
pub mod unix;

#[cfg(feature = "vsock")]
pub mod vsock;

#[cfg(feature = "firecracker")]
pub mod firecracker;

#[allow(unused)]
fn io_input_err(detail: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, detail)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    #[cfg(feature = "tokio-backend")]
    Tokio,
    #[cfg(feature = "async-io-backend")]
    AsyncIo,
}
