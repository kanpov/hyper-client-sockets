use http_body_util::Full;
use hyper::{body::Bytes, client::conn::http1, Request, Uri};
use hyper_client_sockets::{
    unix::{HyperUnixConnector, HyperUnixUri},
    vsock::HyperVsockStream,
};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

#[tokio::test]
async fn unix_test() {
    let client: Client<HyperUnixConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(HyperUnixConnector);
    let uds_uri: Uri = HyperUnixUri::new("/tmp/uds-listener.sock", "/get/ok")
        .unwrap()
        .into();
    let request = Request::builder()
        .uri(uds_uri)
        .body(Full::new(Bytes::new()))
        .unwrap();
    let response = client.request(request).await.unwrap();
    dbg!(response.status());
}

#[tokio::test]
async fn vsock_test() {
    let vsock_stream = HyperVsockStream::connect(1, 8000).await.unwrap();
    let (mut send_req, conn) = http1::Builder::new()
        .handshake::<HyperVsockStream, Full<Bytes>>(vsock_stream)
        .await
        .unwrap();
    tokio::spawn(conn);

    // let client: Client<VsockSocketConnector, Full<Bytes>> =
    //     Client::builder(TokioExecutor::new()).build(VsockSocketConnector);
    // let vsock_uri: Uri = VsockSocketUri::new(1, 8000, "/").unwrap().into();
    let request = Request::builder()
        .uri(Uri::from_static("/"))
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(
            r#"{"first": "a", "middle": "b", "last": "c"}"#,
        )))
        .unwrap();
    let response = send_req.send_request(request).await.unwrap();
    dbg!(response);
}
