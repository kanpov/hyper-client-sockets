use std::{convert::Infallible, path::PathBuf};

use bytes::{Bytes, BytesMut};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Request, Response, Uri};
use hyper_client_sockets::{
    firecracker::{connector::HyperFirecrackerConnector, FirecrackerUriExt, HyperFirecrackerStream},
    unix::{connector::HyperUnixConnector, HyperUnixStream, UnixUriExt},
    vsock::{connector::HyperVsockConnector, HyperVsockStream, VsockUriExt},
    Backend,
};
use hyper_util::{
    client::legacy::Client,
    rt::{TokioExecutor, TokioIo},
};
use rand::Rng;
use tokio::{
    fs::remove_file,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};
use tokio_vsock::{VsockAddr, VsockListener, VMADDR_CID_LOCAL};
use uuid::Uuid;

async fn assert_response_ok(response: &mut Response<Incoming>) {
    assert_eq!(response.status().as_u16(), 200);
    let mut content = BytesMut::new();
    while let Some(next) = response.frame().await {
        if let Some(chunk) = next.unwrap().data_ref() {
            content.extend(chunk);
        }
    }

    assert_eq!(String::from_utf8_lossy(content.as_ref()).into_owned(), "Hello World!");
}

async fn hello_world_route(_: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(Response::new(Full::new(Bytes::from("Hello World!"))))
}

async fn start_unix_server() -> PathBuf {
    let path = PathBuf::from(format!("/tmp/{}", Uuid::new_v4()));

    if path.exists() {
        remove_file(&path).await.unwrap();
    }

    let listener = UnixListener::bind(&path).unwrap();

    tokio::task::spawn(async move {
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
    });

    path
}

async fn start_vsock_server() -> (u32, u32) {
    let port = rand::thread_rng().gen_range(10000..=65536) as u32;
    let mut listener = VsockListener::bind(VsockAddr::new(VMADDR_CID_LOCAL, port)).unwrap();

    tokio::task::spawn(async move {
        let tokio_io = TokioIo::new(listener.accept().await.unwrap().0);
        tokio::task::spawn(async move {
            http1::Builder::new()
                .serve_connection(tokio_io, service_fn(hello_world_route))
                .await
                .unwrap();
        });
    });

    (VMADDR_CID_LOCAL, port)
}

async fn start_firecracker_server() -> (PathBuf, u32) {
    let path = PathBuf::from(format!("/tmp/{}", Uuid::new_v4()));
    if path.exists() {
        remove_file(&path).await.unwrap();
    }
    let guest_port = rand::thread_rng().gen_range(1..=1000) as u32;

    let listener = UnixListener::bind(&path).unwrap();

    tokio::task::spawn(async move {
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
    });

    (path, guest_port)
}

#[tokio::test]
async fn unix_connectivity_with_hyper_util() {
    let path = start_unix_server().await;
    let client: Client<_, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(HyperUnixConnector::new(Backend::Tokio));
    let request = Request::builder()
        .uri(Uri::unix(path, "/").unwrap())
        .method("GET")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let mut response = client.request(request).await.unwrap();
    assert_response_ok(&mut response).await;
}

#[tokio::test]
async fn unix_connectivity_with_raw_hyper() {
    let path = start_unix_server().await;
    let stream = HyperUnixStream::connect(&path, Backend::Tokio).await.unwrap();
    let (mut make_req, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Full<Bytes>>(stream)
        .await
        .unwrap();
    tokio::task::spawn(conn);
    let request = Request::builder()
        .uri("/")
        .method("GET")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let mut response = make_req.send_request(request).await.unwrap();
    assert_response_ok(&mut response).await;
}

#[tokio::test]
async fn vsock_connectivity_with_hyper_util() {
    let (cid, port) = start_vsock_server().await;
    let client: Client<_, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(HyperVsockConnector::new(Backend::Tokio));
    let mut response = client
        .request(
            Request::builder()
                .uri(Uri::vsock(cid, port, "/").unwrap())
                .body(Full::new(Bytes::new()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_response_ok(&mut response).await;
}

#[tokio::test]
async fn vsock_connectivity_with_raw_hyper() {
    let (cid, port) = start_vsock_server().await;
    let stream = HyperVsockStream::connect(VsockAddr::new(cid, port), Backend::Tokio)
        .await
        .unwrap();
    let (mut make_req, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Full<Bytes>>(stream)
        .await
        .unwrap();
    tokio::task::spawn(conn);
    let mut response = make_req
        .send_request(Request::builder().uri("/").body(Full::new(Bytes::new())).unwrap())
        .await
        .unwrap();
    assert_response_ok(&mut response).await;
}

#[tokio::test]
async fn firecracker_connectivity_with_hyper_util() {
    let (path, port) = start_firecracker_server().await;
    let client: Client<_, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(HyperFirecrackerConnector::new(Backend::Tokio));
    let mut response = client
        .request(
            Request::builder()
                .uri(Uri::firecracker(path, port, "/").unwrap())
                .body(Full::new(Bytes::new()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_response_ok(&mut response).await;
}

#[tokio::test]
async fn firecracker_connectivity_with_raw_hyper() {
    let (path, port) = start_firecracker_server().await;
    let (mut make_req, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Full<Bytes>>(
            HyperFirecrackerStream::connect(&path, port, Backend::Tokio)
                .await
                .unwrap(),
        )
        .await
        .unwrap();
    tokio::task::spawn(conn);
    let mut response = make_req
        .send_request(Request::builder().uri("/").body(Full::new(Bytes::new())).unwrap())
        .await
        .unwrap();
    assert_response_ok(&mut response).await;
}
