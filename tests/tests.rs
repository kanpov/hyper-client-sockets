use bytes::Bytes;
use framework::Test;
use http::Request;
use http_body_util::Full;
use hyper::client::conn::http1::handshake;

mod framework;

#[test]
fn unix_connectivity_via_raw_hyper() {
    Test::run(|test| async move {
        let socket_path = test.serve_unix();
        let io = test.connect_unix(&socket_path).await;
        let (mut send_request, connection) = handshake::<_, Full<Bytes>>(io).await.unwrap();
        test.spawn(connection);
        let response = send_request
            .send_request(Request::new(Full::new(Bytes::new())))
            .await
            .unwrap();
        test.check_response(response).await;
    });
}
