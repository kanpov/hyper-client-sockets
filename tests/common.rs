use std::{convert::Infallible, future::Future, path::PathBuf, time::Duration};

use bytes::{Bytes, BytesMut};
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use libc::VMADDR_CID_LOCAL;
use tokio::net::UnixListener;
use tokio_vsock::VsockListener;
use uuid::Uuid;
use vsock::VsockAddr;

#[allow(unused)]
pub fn serve_unix() -> PathBuf {
    let socket_path = PathBuf::from("/tmp").join(Uuid::new_v4().to_string());

    let cloned_socket_path = socket_path.clone();
    in_tokio_thread(async move {
        let listener = UnixListener::bind(cloned_socket_path).unwrap();

        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service_fn(responder))
                    .await
                    .unwrap();
            });
        }
    });

    std::thread::sleep(Duration::from_millis(1));
    socket_path
}

#[allow(unused)]
pub fn serve_vsock() -> VsockAddr {
    let port = fastrand::u32(15000..=65536);
    let addr = VsockAddr::new(VMADDR_CID_LOCAL, port);
    let mut listener = VsockListener::bind(addr).unwrap();

    in_tokio_thread(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service_fn(responder))
                    .await
                    .unwrap();
            });
        }
    });

    std::thread::sleep(Duration::from_millis(10));
    addr
}

#[allow(unused)]
pub async fn check_response(mut response: Response<Incoming>) {
    let mut buf = BytesMut::new();

    while let Some(Ok(frame)) = response.frame().await {
        buf.extend(frame.into_data().unwrap());
    }

    let buf = String::from_utf8(buf.to_vec()).unwrap();
    assert_eq!(buf, "response");
}

async fn responder(_: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(Response::new(Full::new(Bytes::from("response"))))
}

fn in_tokio_thread(future: impl Future + Send + 'static) {
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future);
    });
}
