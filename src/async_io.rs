#[cfg(any(feature = "unix", feature = "firecracker"))]
use std::path::Path;
#[cfg(feature = "vsock")]
use std::{
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd, IntoRawFd},
    pin::Pin,
    task::Poll,
};

use async_io::Async;
#[cfg(feature = "firecracker")]
use futures_lite::{io::BufReader, AsyncBufReadExt, AsyncWriteExt, StreamExt};
#[cfg(any(feature = "unix", feature = "firecracker"))]
use smol_hyper::rt::FuturesIo;
#[cfg(feature = "vsock")]
use vsock::VsockAddr;

use crate::Backend;

#[derive(Debug, Clone)]
pub struct AsyncIoBackend;

impl Backend for AsyncIoBackend {
    type UnixIo = FuturesIo<Async<std::os::unix::net::UnixStream>>;

    #[cfg(feature = "vsock")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
    type VsockIo = AsyncVsockIo;

    type FirecrackerIo = FuturesIo<Async<std::os::unix::net::UnixStream>>;

    async fn connect_to_unix_socket(socket_path: &Path) -> Result<Self::UnixIo, std::io::Error> {
        Ok(FuturesIo::new(
            Async::<std::os::unix::net::UnixStream>::connect(socket_path).await?,
        ))
    }

    #[cfg(feature = "vsock")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
    async fn connect_to_vsock_socket(addr: vsock::VsockAddr) -> Result<Self::VsockIo, std::io::Error> {
        Ok(AsyncVsockIo::connect(addr).await?)
    }

    async fn connect_to_firecracker_socket(
        host_socket_path: &Path,
        guest_port: u32,
    ) -> Result<Self::FirecrackerIo, std::io::Error> {
        let mut stream = Async::<std::os::unix::net::UnixStream>::connect(host_socket_path).await?;
        stream.write_all(format!("CONNECT {guest_port}\n").as_bytes()).await?;

        let mut lines = BufReader::new(&mut stream).lines();
        match lines.next().await {
            Some(Ok(line)) => {
                if !line.starts_with("OK") {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionRefused,
                        "Firecracker refused to establish a tunnel to the given guest port",
                    ));
                }
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Could not read Firecracker response",
                ))
            }
        };

        Ok(FuturesIo::new(stream))
    }
}

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
pub struct AsyncVsockIo(Async<std::fs::File>);

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
impl AsyncVsockIo {
    async fn connect(addr: VsockAddr) -> Result<Self, std::io::Error> {
        let socket = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
        if socket < 0 {
            return Err(std::io::Error::last_os_error());
        }

        if unsafe { libc::fcntl(socket, libc::F_SETFL, libc::O_NONBLOCK | libc::O_CLOEXEC) } < 0 {
            let _ = unsafe { libc::close(socket) };
            return Err(std::io::Error::last_os_error());
        }

        if unsafe {
            libc::connect(
                socket,
                &addr as *const _ as *const libc::sockaddr,
                size_of::<libc::sockaddr_vm>() as libc::socklen_t,
            )
        } < 0
        {
            let err = std::io::Error::last_os_error();
            if let Some(os_err) = err.raw_os_error() {
                if os_err != libc::EINPROGRESS {
                    let _ = unsafe { libc::close(socket) };
                    return Err(err);
                }
            }
        }

        loop {
            let async_fd = Async::new(unsafe { std::fs::File::from_raw_fd(socket) })?;

            let conn_check = async_fd.write_with(|fd| {
                let mut sock_err: libc::c_int = 0;
                let mut sock_err_len: libc::socklen_t = size_of::<libc::c_int>() as libc::socklen_t;
                let err = unsafe {
                    libc::getsockopt(
                        fd.as_raw_fd(),
                        libc::SOL_SOCKET,
                        libc::SO_ERROR,
                        &mut sock_err as *mut _ as *mut libc::c_void,
                        &mut sock_err_len as *mut libc::socklen_t,
                    )
                };

                if err < 0 {
                    return Err(std::io::Error::last_os_error());
                }

                if sock_err == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::from_raw_os_error(sock_err))
                }
            });

            match conn_check.await {
                Ok(_) => {
                    return Ok(AsyncVsockIo(Async::new(unsafe {
                        std::fs::File::from_raw_fd(async_fd.into_inner()?.into_raw_fd())
                    })?))
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::Interrupted =>
                {
                    continue
                }
                Err(err) => return Err(err),
            }
        }
    }
}

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
impl hyper::rt::Write for AsyncVsockIo {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        loop {
            match self.0.poll_writable(cx) {
                Poll::Ready(Ok(guard)) => guard,
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Pending => return Poll::Pending,
            };

            match self.0.get_ref().write(buf) {
                Ok(amount) => return Poll::Ready(Ok(amount)),
                Err(ref err)
                    if err.kind() == std::io::ErrorKind::Interrupted
                        || err.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    continue
                }
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
impl hyper::rt::Read for AsyncVsockIo {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let b;
        unsafe {
            b = &mut *(buf.as_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8]);
        };

        loop {
            match self.0.poll_readable(cx) {
                Poll::Ready(Ok(guard)) => guard,
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Pending => return Poll::Pending,
            };

            match self.0.get_ref().read(b) {
                Ok(amount) => {
                    unsafe {
                        buf.advance(amount);
                    }

                    return Poll::Ready(Ok(()));
                }
                Err(ref err)
                    if err.kind() == std::io::ErrorKind::Interrupted
                        || err.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    continue
                }
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }
}
