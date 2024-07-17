use std::{convert::Infallible, path::PathBuf};

use bytes::Bytes;
use http_body_util::Full;
use hyper::{server::conn::http1, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::{fs::remove_file, net::UnixListener};
use uuid::Uuid;

async fn unix_route(
    _: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(Response::new(Full::new(Bytes::from("Hello World!"))))
}

async fn start_server() -> PathBuf {
    let path = PathBuf::from(format!("/tmp/{}", Uuid::new_v4()));

    if path.exists() {
        remove_file(&path).await.unwrap();
    }

    let listener = UnixListener::bind(&path).unwrap();

    tokio::task::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tokio_io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(tokio_io, service_fn(unix_route))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    });

    path
}

#[tokio::test]
async fn unix_connectivity_with_hyper_util() {}
