use std::{
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd},
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use tokio::io::{unix::AsyncFd, AsyncRead, AsyncWrite};
use vsock::VsockAddr;

// Vendored in and shortened from https://github.com/rust-vsock/tokio-vsock/blob/master/src/stream.rs
// Needed to prevent tokio-vsock dependency from being pulled in when vsock feature is not enabled.

pub struct VsockInternalStream(AsyncFd<vsock::VsockStream>);

impl AsyncWrite for VsockInternalStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
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

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for VsockInternalStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let b;
        unsafe {
            b = &mut *(buf.unfilled_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8]);
        };

        loop {
            let mut guard = ready!(self.0.poll_read_ready(cx))?;

            match guard.try_io(|inner| inner.get_ref().read(b)) {
                Ok(Ok(n)) => {
                    unsafe {
                        buf.assume_init(n);
                    }
                    buf.advance(n);
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

pub async fn connect_to_vsock(addr: VsockAddr) -> std::io::Result<VsockInternalStream> {
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
                return Ok(VsockInternalStream(AsyncFd::new(unsafe {
                    vsock::VsockStream::from_raw_fd(async_fd.into_inner().into_raw_fd())
                })?))
            }
            Ok(Err(err)) => return Err(err),
            Err(_would_block) => continue,
        }
    }
}
