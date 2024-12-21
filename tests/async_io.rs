use std::{future::Future, sync::Arc};

use async_executor::Executor;
use bytes::Bytes;
use common::{check_response, serve_firecracker, serve_unix, serve_vsock};
use http::{Request, Uri};
use http_body_util::Full;
use hyper::client::conn::http1::handshake;
use hyper_client_sockets::{
    async_io::AsyncIoBackend,
    connector::{firecracker::FirecrackerConnector, unix::UnixConnector, vsock::VsockConnector},
    uri::{FirecrackerUri, UnixUri, VsockUri},
    Backend,
};
use hyper_util::client::legacy::Client;
use smol_hyper::rt::SmolExecutor;

mod common;

#[test]
fn async_io_unix_raw_connectivity() {
    run(|executor| async move {
        let socket_path = serve_unix();
        let io = AsyncIoBackend::connect_to_unix_socket(&socket_path).await.unwrap();
        let (mut send_request, conn) = handshake::<_, Full<Bytes>>(io).await.unwrap();
        executor.spawn(conn).detach();
        let response = send_request
            .send_request(Request::new(Full::new(Bytes::new())))
            .await
            .unwrap();
        check_response(response).await;
    });
}

#[test]
fn async_io_unix_pooled_connectivity() {
    run(|executor| async move {
        let socket_path = serve_unix();
        let client = Client::builder(SmolExecutor::new(executor))
            .build::<_, Full<Bytes>>(UnixConnector::<AsyncIoBackend>::new());
        let response = client.get(Uri::unix(&socket_path, "/").unwrap()).await.unwrap();
        check_response(response).await;
    });
}

#[test]
fn async_io_vsock_raw_connectivity() {
    run(|executor| async move {
        let addr = serve_vsock();
        let io = AsyncIoBackend::connect_to_vsock_socket(addr).await.unwrap();
        let (mut send_request, conn) = handshake::<_, Full<Bytes>>(io).await.unwrap();
        executor.spawn(conn).detach();
        let response = send_request
            .send_request(Request::new(Full::new(Bytes::new())))
            .await
            .unwrap();
        check_response(response).await;
    });
}

#[test]
fn async_io_vsock_pooled_connectivity() {
    run(|executor| async move {
        let addr = serve_vsock();
        let client = Client::builder(SmolExecutor::new(executor))
            .build::<_, Full<Bytes>>(VsockConnector::<AsyncIoBackend>::new());
        let response = client.get(Uri::vsock(addr, "/").unwrap()).await.unwrap();
        check_response(response).await;
    });
}

#[test]
fn async_io_firecracker_raw_connectivity() {
    run(|executor| async move {
        let socket_path = serve_firecracker();
        let io = AsyncIoBackend::connect_to_firecracker_socket(&socket_path, 1234)
            .await
            .unwrap();
        let (mut send_request, conn) = handshake::<_, Full<Bytes>>(io).await.unwrap();
        executor.spawn(conn).detach();
        let response = send_request
            .send_request(Request::new(Full::new(Bytes::new())))
            .await
            .unwrap();
        check_response(response).await;
    });
}

#[test]
fn async_io_firecracker_pooled_connectivity() {
    run(|executor| async move {
        let socket_path = serve_firecracker();
        let client = Client::builder(SmolExecutor::new(executor))
            .build::<_, Full<Bytes>>(FirecrackerConnector::<AsyncIoBackend>::new());
        let response = client
            .get(Uri::firecracker(&socket_path, 1234, "/").unwrap())
            .await
            .unwrap();
        check_response(response).await;
    });
}

fn run<F: FnOnce(Arc<Executor<'static>>) -> Fut, Fut: Future>(function: F) {
    let executor = Arc::new(Executor::new());
    async_io::block_on(executor.clone().run(function(executor)));
}
