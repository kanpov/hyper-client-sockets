use bytes::Bytes;
use common::{assert_response_ok, start_firecracker_server, start_unix_server, start_vsock_server};
use http_body_util::Full;
use hyper::{Request, Uri};
use hyper_client_sockets::{
    firecracker::{connector::HyperFirecrackerConnector, FirecrackerUriExt, HyperFirecrackerStream},
    unix::{connector::HyperUnixConnector, HyperUnixStream, UnixUriExt},
    vsock::{connector::HyperVsockConnector, HyperVsockStream, VsockUriExt},
    Backend,
};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use vsock::VsockAddr;

mod common;

#[tokio::test]
async fn unix_connectivity_with_hyper_util() {
    let path = start_unix_server();
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(HyperUnixConnector {
        backend: Backend::Tokio,
    });
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
    let path = start_unix_server();
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
    let (cid, port) = start_vsock_server();
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(HyperVsockConnector {
        backend: Backend::Tokio,
    });
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
    let (cid, port) = start_vsock_server();
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
    let (path, port) = start_firecracker_server();
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(HyperFirecrackerConnector {
        backend: Backend::Tokio,
    });
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
    let (path, port) = start_firecracker_server();
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
