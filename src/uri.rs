#[cfg(any(feature = "unix", feature = "vsock", feature = "firecracker"))]
use hex::FromHex;
#[cfg(any(feature = "unix", feature = "vsock", feature = "firecracker"))]
use http::{uri::InvalidUri, Uri};
#[cfg(any(feature = "unix", feature = "vsock", feature = "firecracker"))]
use std::path::{Path, PathBuf};

/// An extension trait for a URI that allows constructing a hex-encoded Unix socket URI.
#[cfg(feature = "unix")]
#[cfg_attr(docsrs, doc(cfg(feature = "unix")))]
pub trait UnixUri {
    /// Create a new Unix URI with the given socket path and in-socket URI.
    fn unix(socket_path: impl AsRef<Path>, url: impl AsRef<str>) -> Result<Uri, InvalidUri>;

    /// Try to deconstruct this Unix URI's socket path.
    fn parse_unix(&self) -> Result<PathBuf, std::io::Error>;
}

#[cfg(feature = "unix")]
#[cfg_attr(docsrs, doc(cfg(feature = "unix")))]
impl UnixUri for Uri {
    fn unix(socket_path: impl AsRef<Path>, url: impl AsRef<str>) -> Result<Uri, InvalidUri> {
        let host = hex::encode(socket_path.as_ref().to_string_lossy().to_string());
        let uri_str = format!("unix://{host}/{}", url.as_ref().trim_start_matches('/'));
        let uri = uri_str.parse::<Uri>()?;
        Ok(uri)
    }

    fn parse_unix(&self) -> Result<PathBuf, std::io::Error> {
        if self.scheme_str() != Some("unix") {
            return Err(io_input_err("URI scheme on a Unix socket must be unix://"));
        }

        match self.host() {
            Some(host) => {
                let bytes = Vec::from_hex(host).map_err(|_| io_input_err("URI host must be hex"))?;
                Ok(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
            }
            None => Err(io_input_err("URI host must be present")),
        }
    }
}

/// An extension trait for hyper URI that allows constructing a hex-encoded virtio-vsock socket URI.
#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
pub trait VsockUri {
    /// Create a new vsock URI with the given vsock CID, port and in-socket URL
    fn vsock(cid: u32, port: u32, url: impl AsRef<str>) -> Result<Uri, InvalidUri>;

    /// Deconstruct this vsock URI into its address.
    fn parse_vsock(&self) -> Result<vsock::VsockAddr, std::io::Error>;
}

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
impl VsockUri for Uri {
    fn vsock(cid: u32, port: u32, url: impl AsRef<str>) -> Result<Uri, InvalidUri> {
        let host = hex::encode(format!("{cid}.{port}"));
        let uri_str = format!("vsock://{host}/{}", url.as_ref().trim_start_matches('/'));
        let uri = uri_str.parse::<Uri>()?;
        Ok(uri)
    }

    fn parse_vsock(&self) -> Result<vsock::VsockAddr, std::io::Error> {
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

                Ok(vsock::VsockAddr::new(cid, port))
            }
            None => Err(io_input_err("URI host must be present")),
        }
    }
}

/// An extension trait for hyper URI that allows constructing a hex-encoded Firecracker socket URI.
#[cfg(feature = "firecracker")]
#[cfg_attr(docsrs, doc(cfg(feature = "firecracker")))]
pub trait FirecrackerUri {
    /// Create a new Firecracker URI with the given host socket path, guest port and in-socket URL
    fn firecracker(
        host_socket_path: impl AsRef<Path>,
        guest_port: u32,
        url: impl AsRef<str>,
    ) -> Result<Uri, InvalidUri>;

    /// Deconstruct this Firecracker URI into its host socket path and guest port
    fn parse_firecracker(&self) -> Result<(PathBuf, u32), std::io::Error>;
}

#[cfg(feature = "firecracker")]
#[cfg_attr(docsrs, doc(cfg(feature = "firecracker")))]
impl FirecrackerUri for Uri {
    fn firecracker(
        host_socket_path: impl AsRef<Path>,
        guest_port: u32,
        url: impl AsRef<str>,
    ) -> Result<Uri, InvalidUri> {
        let host = hex::encode(format!(
            "{}:{guest_port}",
            host_socket_path.as_ref().to_string_lossy().to_string()
        ));
        let uri_str = format!("fc://{host}/{}", url.as_ref().trim_start_matches('/'));
        let uri = uri_str.parse::<Uri>()?;
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

#[cfg(any(feature = "unix", feature = "vsock", feature = "firecracker"))]
#[inline(always)]
fn io_input_err(detail: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, detail)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hyper::Uri;
    use vsock::VsockAddr;

    use crate::uri::{FirecrackerUri, UnixUri, VsockUri};

    #[test]
    fn unix_uri_should_be_constructed_correctly() {
        let uri_str = format!("unix://{}/route", hex::encode("/tmp/socket.sock"));
        assert_eq!(
            Uri::unix("/tmp/socket.sock", "/route").unwrap(),
            uri_str.parse::<Uri>().unwrap()
        );
    }

    #[test]
    fn unix_uri_should_be_deconstructed_correctly() {
        let uri = format!("unix://{}/route", hex::encode("/tmp/socket.sock"));
        assert_eq!(
            uri.parse::<Uri>().unwrap().parse_unix().unwrap(),
            PathBuf::from("/tmp/socket.sock")
        );
    }

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
        assert_eq!(uri.parse_vsock().unwrap(), VsockAddr::new(10, 20));
    }

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
