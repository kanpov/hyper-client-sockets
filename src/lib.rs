#[cfg(feature = "unix")]
mod unix;
#[cfg(feature = "unix")]
pub use unix::*;

#[cfg(feature = "vsock")]
mod vsock;
#[cfg(feature = "vsock")]
pub use vsock::*;

#[cfg(feature = "firecracker")]
mod firecracker;
#[cfg(feature = "firecracker")]
pub use firecracker::*;

#[allow(unused)]
fn io_input_err(detail: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, detail)
}
