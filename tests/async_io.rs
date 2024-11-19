use std::{future::Future, sync::Arc};

use async_executor::Executor;
use async_io::block_on;
use bytes::Bytes;
use common::{assert_response_ok, start_unix_server};
use http::{Request, Uri};
use http_body_util::Full;
use hyper_client_sockets::{
    unix::{connector::HyperUnixConnector, UnixUriExt},
    Backend,
};
use hyper_util::client::legacy::Client;
use smol_hyper::rt::SmolExecutor;

mod common;

#[test]
fn unix_connectivity_with_hyper_util() {
    run_test(|executor| async {
        let path = start_unix_server();
        let client: Client<_, Full<Bytes>> =
            Client::builder(SmolExecutor::new(executor)).build(HyperUnixConnector::new(Backend::AsyncIo));
        let request = Request::builder()
            .uri(Uri::unix(path, "/").unwrap())
            .method("GET")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let mut response = client.request(request).await.unwrap();
        assert_response_ok(&mut response).await;
    });
}

fn run_test<F, Fut>(function: F)
where
    F: FnOnce(Arc<Executor<'static>>) -> Fut,
    Fut: Future<Output = ()>,
{
    let executor = Arc::new(Executor::new());
    block_on(executor.clone().run(function(executor)));
}
