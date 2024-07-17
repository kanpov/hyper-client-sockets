#[cfg(feature = "unix")]
pub mod unix;

#[cfg(feature = "vsock")]
pub mod vsock;

fn io_input_err(detail: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, detail)
}
