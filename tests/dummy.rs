use http_body_util::Full;
use hyper::{body::Bytes, Request, Uri};
use hyper_client_sockets::unix::{UnixSocketConnector, UnixSocketUri};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

#[tokio::test]
async fn unix_test() {
    let client: Client<UnixSocketConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(UnixSocketConnector);
    let uds_uri: Uri = UnixSocketUri::new("/tmp/uds-listener.sock", "/get/ok")
        .unwrap()
        .into();
    let request = Request::builder()
        .uri(uds_uri)
        .body(Full::new(Bytes::from("")))
        .unwrap();
    let response = client.request(request).await.unwrap();
    dbg!(response.status());
}
