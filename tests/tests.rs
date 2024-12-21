use framework::Test;

mod framework;

#[test]
fn unix_connectivity_via_raw_hyper() {
    Test::run(|test| async move {
        let socket_path = test.serve_unix();
        let io = test.connect_unix(&socket_path).await;
        test.test_raw(io).await;
    });
}
