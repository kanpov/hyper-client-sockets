use std::{convert::Infallible, future::Future, path::PathBuf};

use bytes::{Bytes, BytesMut};
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use rand::Rng;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};
use tokio_vsock::VsockListener;
use uuid::Uuid;
use vsock::{VsockAddr, VMADDR_CID_LOCAL};

pub async fn assert_response_ok(response: &mut Response<Incoming>) {
    assert_eq!(response.status().as_u16(), 200);
    let mut content = BytesMut::new();
    while let Some(next) = response.frame().await {
        if let Some(chunk) = next.unwrap().data_ref() {
            content.extend(chunk);
        }
    }

    assert_eq!(String::from_utf8_lossy(content.as_ref()).into_owned(), "Hello World!");
}

pub async fn hello_world_route(_: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(Response::new(Full::new(Bytes::from("Hello World!"))))
}

pub fn start_unix_server() -> PathBuf {
    let path = PathBuf::from(format!("/tmp/{}", Uuid::new_v4()));

    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }

    let listener = UnixListener::bind(&path).unwrap();

    in_tokio(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let tokio_io = TokioIo::new(stream);

            tokio::task::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .serve_connection(tokio_io, service_fn(hello_world_route))
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }
    });

    path
}

pub fn start_vsock_server() -> (u32, u32) {
    let port = rand::thread_rng().gen_range(10000..=65536) as u32;
    let mut listener = VsockListener::bind(VsockAddr::new(VMADDR_CID_LOCAL, port)).unwrap();

    in_tokio(async move {
        loop {
            let tokio_io = TokioIo::new(listener.accept().await.unwrap().0);
            tokio::task::spawn(async move {
                http1::Builder::new()
                    .serve_connection(tokio_io, service_fn(hello_world_route))
                    .await
                    .unwrap();
            });
        }
    });

    (VMADDR_CID_LOCAL, port)
}

pub fn start_firecracker_server() -> (PathBuf, u32) {
    let path = PathBuf::from(format!("/tmp/{}", Uuid::new_v4()));
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }
    let guest_port = rand::thread_rng().gen_range(1..=1000) as u32;

    let listener = UnixListener::bind(&path).unwrap();

    in_tokio(async move {
        loop {
            // Recreate the CONNECT behavior of a real Firecracker socket
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf_reader = BufReader::new(&mut stream).lines();
            let mut line = String::new();
            buf_reader.get_mut().read_line(&mut line).await.unwrap();

            if line == format!("CONNECT {guest_port}\n") {
                stream.write_all(b"OK\n").await.unwrap();
            } else {
                stream.write_all(b"REJECTED\n").await.unwrap();
                return;
            }

            // After sending out approval, serve HTTP
            let tokio_io = TokioIo::new(stream);
            tokio::task::spawn(async move {
                http1::Builder::new()
                    .serve_connection(tokio_io, service_fn(hello_world_route))
                    .await
                    .unwrap();
            });
        }
    });

    (path, guest_port)
}

fn in_tokio(future: impl Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    });
}
