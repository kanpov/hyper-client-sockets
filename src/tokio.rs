#[cfg(feature = "vsock")]
use std::{
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd},
    task::Poll,
};

#[cfg(feature = "unix")]
use hyper_util::rt::TokioIo;
#[cfg(feature = "unix")]
use tokio::net::UnixStream;

#[cfg(feature = "vsock")]
use futures_util::ready;
#[cfg(feature = "vsock")]
use tokio::io::unix::AsyncFd;

#[cfg(feature = "unix")]
use std::path::Path;

use crate::Backend;

/// [Backend] for hyper-client-sockets that is implemented via Tokio's I/O.
pub struct TokioBackend;

impl Backend for TokioBackend {
    #[cfg(feature = "unix")]
    #[cfg_attr(docsrs, doc(cfg(feature = "unix")))]
    type UnixIo = TokioIo<UnixStream>;

    #[cfg(feature = "vsock")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
    type VsockIo = TokioVsockIo;

    #[cfg(feature = "unix")]
    #[cfg_attr(docsrs, doc(cfg(feature = "unix")))]
    async fn connect_to_unix_socket(socket_path: &Path) -> Result<Self::UnixIo, std::io::Error> {
        Ok(TokioIo::new(UnixStream::connect(socket_path).await?))
    }

    #[cfg(feature = "vsock")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
    async fn connect_to_vsock_socket(addr: vsock::VsockAddr) -> Result<Self::VsockIo, std::io::Error> {
        TokioVsockIo::connect(addr).await
    }
}

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
pub struct TokioVsockIo(AsyncFd<vsock::VsockStream>);

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
impl TokioVsockIo {
    async fn connect(addr: vsock::VsockAddr) -> Result<Self, std::io::Error> {
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
                // Connect hasn't finished, that's fine.
                if os_err != libc::EINPROGRESS {
                    // Close the socket if we hit an error, ignoring the error
                    // from closing since we can't pass back two errors.
                    let _ = unsafe { libc::close(socket) };
                    return Err(err);
                }
            }
        }

        loop {
            let async_fd = AsyncFd::new(unsafe { OwnedFd::from_raw_fd(socket) })?;
            let mut guard = async_fd.writable().await?;

            // Checks if the connection failed or not
            let conn_check = guard.try_io(|fd| {
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

            match conn_check {
                Ok(Ok(_)) => {
                    return Ok(TokioVsockIo(AsyncFd::new(unsafe {
                        vsock::VsockStream::from_raw_fd(async_fd.into_inner().into_raw_fd())
                    })?))
                }
                Ok(Err(err)) => return Err(err),
                Err(_would_block) => continue,
            }
        }
    }
}

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
impl hyper::rt::Write for TokioVsockIo {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        loop {
            let mut guard = ready!(self.0.poll_write_ready(cx))?;

            match guard.try_io(|inner| inner.get_ref().write(buf)) {
                Ok(Ok(n)) => return Ok(n).into(),
                Ok(Err(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Ok(Err(e)) => return Err(e).into(),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(feature = "vsock")]
#[cfg_attr(docsrs, doc(cfg(feature = "vsock")))]
impl hyper::rt::Read for TokioVsockIo {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let b;
        unsafe {
            b = &mut *(buf.as_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8]);
        };

        loop {
            let mut guard = ready!(self.0.poll_read_ready(cx))?;

            match guard.try_io(|inner| inner.get_ref().read(b)) {
                Ok(Ok(n)) => {
                    unsafe {
                        buf.advance(n);
                    }

                    return Ok(()).into();
                }
                Ok(Err(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Ok(Err(e)) => return Err(e).into(),
                Err(_would_block) => {
                    continue;
                }
            }
        }
    }
}
