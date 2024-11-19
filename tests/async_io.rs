use std::{future::Future, sync::Arc};

use async_executor::Executor;
use async_io::block_on;
use bytes::Bytes;
use common::{assert_response_ok, start_firecracker_server, start_unix_server, start_vsock_server};
use http::{Request, Uri};
use http_body_util::Full;
use hyper_client_sockets::{
    firecracker::{connector::HyperFirecrackerConnector, FirecrackerUriExt, HyperFirecrackerStream},
    unix::{connector::HyperUnixConnector, HyperUnixStream, UnixUriExt},
    vsock::{connector::HyperVsockConnector, HyperVsockStream, VsockUriExt},
    Backend,
};
use hyper_util::client::legacy::Client;
use smol_hyper::rt::SmolExecutor;
use vsock::VsockAddr;

mod common;

#[test]
fn unix_connectivity_with_hyper_util() {
    run_test(|executor| async {
        let path = start_unix_server();
        let client: Client<_, Full<Bytes>> = Client::builder(SmolExecutor::new(executor)).build(HyperUnixConnector {
            backend: Backend::AsyncIo,
        });
        let request = Request::builder()
            .uri(Uri::unix(path, "/").unwrap())
            .method("GET")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let mut response = client.request(request).await.unwrap();
        assert_response_ok(&mut response).await;
    });
}

#[test]
fn unix_connectivity_with_raw_hyper() {
    run_test(|executor| async move {
        let path = start_unix_server();
        let stream = HyperUnixStream::connect(&path, Backend::AsyncIo).await.unwrap();
        let (mut make_req, conn) = hyper::client::conn::http1::Builder::new()
            .handshake::<_, Full<Bytes>>(stream)
            .await
            .unwrap();
        executor.spawn(conn).detach();
        let request = Request::builder()
            .uri("/")
            .method("GET")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let mut response = make_req.send_request(request).await.unwrap();
        assert_response_ok(&mut response).await;
    });
}

#[test]
fn vsock_connectivity_with_hyper_util() {
    run_test(|executor| async {
        let (cid, port) = start_vsock_server();
        let client: Client<_, Full<Bytes>> = Client::builder(SmolExecutor::new(executor)).build(HyperVsockConnector {
            backend: Backend::AsyncIo,
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
    });
}

#[test]
fn vsock_connectivity_with_raw_hyper() {
    run_test(|executor| async move {
        let (cid, port) = start_vsock_server();
        let stream = HyperVsockStream::connect(VsockAddr::new(cid, port), Backend::AsyncIo)
            .await
            .unwrap();
        let (mut make_req, conn) = hyper::client::conn::http1::Builder::new()
            .handshake::<_, Full<Bytes>>(stream)
            .await
            .unwrap();
        executor.spawn(conn).detach();
        let mut response = make_req
            .send_request(Request::builder().uri("/").body(Full::new(Bytes::new())).unwrap())
            .await
            .unwrap();
        assert_response_ok(&mut response).await;
    });
}

#[test]
fn firecracker_connectivity_with_hyper_util() {
    run_test(|executor| async {
        let (path, port) = start_firecracker_server();
        let client: Client<_, Full<Bytes>> =
            Client::builder(SmolExecutor::new(executor)).build(HyperFirecrackerConnector {
                backend: Backend::AsyncIo,
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
    });
}

#[test]
fn firecracker_connectivity_with_raw_hyper() {
    run_test(|executor| async move {
        let (path, port) = start_firecracker_server();
        let (mut make_req, conn) = hyper::client::conn::http1::Builder::new()
            .handshake::<_, Full<Bytes>>(
                HyperFirecrackerStream::connect(&path, port, Backend::AsyncIo)
                    .await
                    .unwrap(),
            )
            .await
            .unwrap();
        executor.spawn(conn).detach();
        let mut response = make_req
            .send_request(Request::builder().uri("/").body(Full::new(Bytes::new())).unwrap())
            .await
            .unwrap();
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