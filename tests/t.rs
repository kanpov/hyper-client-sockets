use std::pin::Pin;

use futures_lite::future::poll_fn;
use hyper::rt::{Read, ReadBuf, Write};
use hyper_client_sockets::{async_io::AsyncIoBackend, Backend};
use vsock::VsockAddr;

#[test]
fn t() {
    async_io::block_on(async {
        let mut io = AsyncIoBackend::connect_to_vsock_socket(VsockAddr::new(1, 8080))
            .await
            .unwrap();
        // let buf_str = String::from("hello\n");
        // let buf = buf_str.as_bytes();
        // poll_fn(|cx| Pin::new(&mut io).poll_write(cx, buf)).await.unwrap();

        loop {
            let mut raw = Vec::new();
            let mut buf = ReadBuf::new(&mut raw);
            poll_fn(|cx| Pin::new(&mut io).poll_read(cx, buf.unfilled()))
                .await
                .unwrap();
            dbg!(raw);
        }
    });
}
