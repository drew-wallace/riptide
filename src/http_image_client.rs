use std::sync::Arc;

use anyhow::Result;
use futures::future::BoxFuture;
use gpui::http_client::{
    AsyncBody, HttpClient, RedirectPolicy, Url,
    http::{self, HeaderValue, Response, StatusCode},
};

/// Minimal reqwest-backed `HttpClient` for GPUI so `img("https://...")`
/// natively fetches, decodes, and GPU-caches album covers and artist portraits.
pub struct ReqwestImageClient {
    client: reqwest::Client,
    runtime: Arc<tokio::runtime::Runtime>,
    user_agent: HeaderValue,
}

impl ReqwestImageClient {
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ytm-img-http")
            .enable_all()
            .build()?;

        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36")
            .pool_max_idle_per_host(8)
            .build()?;

        Ok(Self {
            client,
            runtime: Arc::new(runtime),
            user_agent: HeaderValue::from_static("riptide/0.1"),
        })
    }
}

impl HttpClient for ReqwestImageClient {
    fn type_name(&self) -> &'static str {
        "ReqwestImageClient"
    }

    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        req: http::Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>>> {
        let client = self.client.clone();
        let runtime = self.runtime.clone();

        Box::pin(async move {
            let uri = req.uri().to_string();
            let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes())
                .unwrap_or(reqwest::Method::GET);

            let _redirect_policy = req
                .extensions()
                .get::<RedirectPolicy>()
                .cloned()
                .unwrap_or(RedirectPolicy::FollowAll);

            let headers: Vec<(String, Vec<u8>)> = req
                .headers()
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), v.as_bytes().to_vec()))
                .collect();

            let (tx, rx) = futures::channel::oneshot::channel();
            runtime.spawn(async move {
                let result: Result<(u16, Vec<(String, Vec<u8>)>, Vec<u8>)> = async {
                    let mut builder = client.request(method, &uri);
                    for (k, v) in headers {
                        if let Ok(val) = reqwest::header::HeaderValue::from_bytes(&v) {
                            builder = builder.header(&k, val);
                        }
                    }

                    let resp = builder.send().await?;
                    let status = resp.status().as_u16();
                    let resp_headers = resp
                        .headers()
                        .iter()
                        .map(|(k, v)| (k.as_str().to_string(), v.as_bytes().to_vec()))
                        .collect();
                    let bytes = resp.bytes().await?.to_vec();
                    Ok((status, resp_headers, bytes))
                }
                .await;

                let _ = tx.send(result);
            });

            let (status, resp_headers, bytes) = rx.await??;
            let mut response = Response::builder()
                .status(StatusCode::from_u16(status).unwrap_or(StatusCode::OK));

            for (k, v) in resp_headers {
                if let Ok(val) = HeaderValue::from_bytes(&v) {
                    response = response.header(k.as_str(), val);
                }
            }

            Ok(response.body(AsyncBody::from(bytes))?)
        })
    }
}
