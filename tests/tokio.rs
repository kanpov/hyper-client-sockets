use bytes::Bytes;
use common::{check_response, serve_unix, serve_vsock};
use http::{Request, Uri};
use http_body_util::Full;
use hyper::client::conn::http1::handshake;
use hyper_client_sockets::{connector::unix::UnixConnector, tokio::TokioBackend, uri::UnixUri, Backend};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

mod common;

#[tokio::test]
async fn tokio_unix_raw_connectivity() {
    let socket_path = serve_unix();
    let io = TokioBackend::connect_to_unix_socket(&socket_path).await.unwrap();
    let (mut send_request, conn) = handshake::<_, Full<Bytes>>(io).await.unwrap();
    tokio::spawn(conn);
    let response = send_request
        .send_request(Request::new(Full::new(Bytes::new())))
        .await
        .unwrap();
    check_response(response).await;
}

#[tokio::test]
async fn tokio_unix_pooled_connectivity() {
    let socket_path = serve_unix();
    let client = Client::builder(TokioExecutor::new()).build::<_, Full<Bytes>>(UnixConnector::<TokioBackend>::new());
    let response = client.get(Uri::unix(&socket_path, "/").unwrap()).await.unwrap();
    check_response(response).await;
}

#[tokio::test]
async fn tokio_vsock_raw_connectivity() {
    let addr = serve_vsock();
    let io = TokioBackend::connect_to_vsock_socket(addr).await.unwrap();
    let (mut send_request, conn) = handshake::<_, Full<Bytes>>(io).await.unwrap();
    tokio::spawn(conn);
    let response = send_request
        .send_request(Request::new(Full::new(Bytes::new())))
        .await
        .unwrap();
    check_response(response).await;
}
