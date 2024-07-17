use std::{convert::Infallible, path::PathBuf};

use bytes::{Bytes, BytesMut};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Request, Response, Uri};
use hyper_client_sockets::{
    HyperUnixConnector, HyperUnixStream, HyperUnixUri, HyperVsockConnector, HyperVsockStream, HyperVsockUri,
};
use hyper_util::{
    client::legacy::Client,
    rt::{TokioExecutor, TokioIo},
};
use rand::Rng;
use tokio::{fs::remove_file, net::UnixListener};
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

#[tokio::test]
async fn unix_connectivity_with_hyper_util() {
    let path = start_unix_server().await;
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(HyperUnixConnector);
    let unix_uri: Uri = HyperUnixUri::new(path, "/").unwrap().into();
    let request = Request::builder()
        .uri(unix_uri)
        .method("GET")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let mut response = client.request(request).await.unwrap();
    assert_response_ok(&mut response).await;
}

#[tokio::test]
async fn unix_connectivity_with_raw_hyper() {
    let path = start_unix_server().await;
    let stream = HyperUnixStream::connect(&path).await.unwrap();
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
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(HyperVsockConnector);
    let vsock_uri: Uri = HyperVsockUri::new(cid, port, "/").unwrap().into();
    let mut response = client
        .request(Request::builder().uri(vsock_uri).body(Full::new(Bytes::new())).unwrap())
        .await
        .unwrap();
    assert_response_ok(&mut response).await;
}

#[tokio::test]
async fn vsock_connectivity_with_raw_hyper() {
    let (cid, port) = start_vsock_server().await;
    let stream = HyperVsockStream::connect(cid, port).await.unwrap();
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
