use std::{
    convert::Infallible,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use async_executor::Executor;
use bytes::{Bytes, BytesMut};
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use hyper::{
    body::Incoming,
    rt::{Read, Write},
    service::service_fn,
};
use hyper_client_sockets::{async_io::AsyncIoBackend, tokio::TokioBackend, Backend};
use hyper_util::rt::TokioIo;
use smol_hyper::rt::FuturesIo;
use uuid::Uuid;
use vsock::VsockAddr;

#[derive(Clone)]
pub enum Test {
    Tokio,
    AsyncIo(Arc<Executor<'static>>),
}

#[allow(unused)]
impl Test {
    pub fn run<F: Fn(Test) -> Fut, Fut: Future>(function: F) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(function(Test::Tokio));

        let executor = Arc::new(Executor::new());
        async_io::block_on(executor.clone().run(function(Test::AsyncIo(executor))));
    }

    pub fn spawn<F: Future<Output = O> + Send + 'static, O: Send + 'static>(&self, future: F) {
        match self {
            Test::Tokio => {
                tokio::task::spawn(future);
            }
            Test::AsyncIo(ref executor) => {
                executor.spawn(future).detach();
            }
        }
    }

    pub fn serve_unix(&self) -> PathBuf {
        let socket_path = PathBuf::from("/tmp").join(Uuid::new_v4().to_string());
        let test = self.clone();

        match self {
            Test::Tokio => {
                let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
                self.spawn(async move {
                    loop {
                        let (stream, _) = listener.accept().await.unwrap();
                        test.serve_http_conn(Box::new(TokioIo::new(stream)));
                    }
                });
            }
            Test::AsyncIo(_) => {
                let listener = async_io::Async::<std::os::unix::net::UnixListener>::bind(&socket_path).unwrap();
                self.spawn(async move {
                    loop {
                        let (stream, _) = listener.accept().await.unwrap();
                        test.serve_http_conn(Box::new(FuturesIo::new(stream)));
                    }
                });
            }
        }

        socket_path
    }

    pub async fn connect_unix(&self, path: &Path) -> Box<dyn HyperIo> {
        match self {
            Test::Tokio => Box::new(TokioBackend::connect_to_unix_socket(path).await.unwrap()),
            Test::AsyncIo(_) => Box::new(AsyncIoBackend::connect_to_unix_socket(path).await.unwrap()),
        }
    }

    pub async fn connect_vsock(&self, addr: VsockAddr) -> Box<dyn HyperIo> {
        match self {
            Test::Tokio => Box::new(TokioBackend::connect_to_vsock_socket(addr).await.unwrap()),
            Test::AsyncIo(_) => Box::new(AsyncIoBackend::connect_to_vsock_socket(addr).await.unwrap()),
        }
    }

    pub async fn connect_firecracker(&self, host_socket_path: &Path, guest_port: u32) -> Box<dyn HyperIo> {
        match self {
            Test::Tokio => Box::new(
                TokioBackend::connect_to_firecracker_socket(host_socket_path, guest_port)
                    .await
                    .unwrap(),
            ),
            Test::AsyncIo(_) => Box::new(
                AsyncIoBackend::connect_to_firecracker_socket(host_socket_path, guest_port)
                    .await
                    .unwrap(),
            ),
        }
    }

    pub async fn check_response(&self, mut response: Response<Incoming>) {
        let mut buf = BytesMut::new();

        while let Some(Ok(frame)) = response.frame().await {
            buf.extend(frame.into_data().unwrap());
        }

        let text = String::from_utf8(buf.to_vec()).unwrap();
        assert_eq!(text, "response");
    }

    fn serve_http_conn(&self, io: Box<dyn HyperIo>) {
        self.spawn(async move {
            hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service_fn(responder_route))
                .await
                .expect("Could not serve HTTP/1 connection");
        });
    }
}

#[derive(Clone)]
pub struct TestHyperExecutor(pub Test);

impl hyper::rt::Executor<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> for TestHyperExecutor {
    fn execute(&self, fut: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        self.0.spawn(fut);
    }
}

pub trait HyperIo: Read + Write + Unpin + Send {}

impl<IO: Read + Write + Unpin + Send> HyperIo for IO {}

async fn responder_route(_: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(Response::new(Full::new(Bytes::from("response"))))
}
