mod options;
mod signals;
use anyhow::anyhow;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use hyper_tls::HttpsConnector;
use log::{error, info};
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::str::from_utf8;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use tokio::time::timeout;

async fn proxy_request(server_request: Request<Body>) -> anyhow::Result<Response<Body>> {
    let start = Instant::now();
    let params = server_request
        .uri()
        .query()
        .map(|v| {
            url::form_urlencoded::parse(v.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_else(HashMap::new);

    let client = hyper::Client::builder().build::<_, hyper::Body>(HttpsConnector::new());
    let url = params.get("url").ok_or(anyhow!("parse url"))?;
    let mut client_request = Request::builder().method(server_request.method()).uri(url);
    for header in [
        hyper::header::AUTHORIZATION,
        hyper::header::CONTENT_TYPE,
        hyper::header::USER_AGENT,
        hyper::header::ACCEPT,
    ] {
        if let Some(value) = server_request.headers().get(&header) {
            client_request = client_request.header(header, value);
        }
    }

    let response = client
        .request(client_request.body(server_request.into_body())?)
        .await?;
    let status_code = response.status().as_u16();
    let response_headers = response
        .headers()
        .iter()
        .map(|(k, v)| Ok((k.as_str().to_owned(), v.to_str()?.to_owned())))
        .collect::<anyhow::Result<HashMap<String, String>>>()?;
    let content_bytes = hyper::body::to_bytes(response.into_body()).await?.to_vec();
    let content_string = from_utf8(&content_bytes)?;

    Ok(Response::builder()
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .header(hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from(
            json!({
                "status": {
                    "url": url,
                    "http_code": status_code,
                    "headers": response_headers,
                },
                "contents": content_string,
                "response_time": start.elapsed().as_millis() as u64,
            })
            .to_string(),
        ))?)
}

async fn proxy_handler(request: Request<Body>) -> Result<Response<Body>, Infallible> {
    info!("{}", request.uri());

    if request.uri().path() == "/get" {
        let request = timeout(Duration::from_secs(3), proxy_request(request));
        if let Ok(Ok(response)) = request.await {
            return Ok(response);
        }
    };

    Ok(Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Body::empty())
        .unwrap())
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pretty_env_logger::init();
    let opt = options::Opt::from_args();

    let addr = ([0, 0, 0, 0], opt.port).into();
    let server = Server::bind(&addr).serve(make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(proxy_handler))
    }));

    info!("Listening on http://{}", addr);
    let graceful = server.with_graceful_shutdown(signals::shutdown_signal());
    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    };

    Ok(())
}
