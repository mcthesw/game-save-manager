use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::*;
use futures_util::StreamExt;

const MISSING: &str = r#"<?xml version="1.0"?><d:error xmlns:d="DAV:" xmlns:s="http://ns.jianguoyun.com"><s:exception>AncestorsNotFound</s:exception><s:message>Missing ancestors</s:message></d:error>"#;

struct Fetcher {
    status: StatusCode,
    bytes: Vec<u8>,
    polls: Arc<AtomicUsize>,
}

impl HttpFetch for Fetcher {
    async fn fetch(&self, request: Request<Buffer>) -> opendal::Result<Response<HttpBody>> {
        assert_eq!(request.headers()["authorization"], "Basic unchanged");
        assert_eq!(request.uri(), "http://localhost/namespace.json");
        let bytes = self.bytes.clone();
        let polls = self.polls.clone();
        let body = HttpBody::new(
            futures_util::stream::once(std::future::ready(Ok(Buffer::from(bytes)))).inspect(
                move |_| {
                    polls.fetch_add(1, Ordering::SeqCst);
                },
            ),
            Some(self.bytes.len() as u64),
        );
        Ok(Response::builder()
            .status(self.status)
            .header("x-provider", "unchanged")
            .body(body)
            .unwrap())
    }
}

async fn try_fetch(
    method: &str,
    status: StatusCode,
    bytes: &[u8],
) -> (opendal::Result<Response<HttpBody>>, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    let client = WebDavHttpClient {
        inner: HttpClient::with(Fetcher {
            status,
            bytes: bytes.to_vec(),
            polls: polls.clone(),
        }),
        cooldown: Box::leak(Box::new(Cooldown::new())),
    };
    let request = Request::builder()
        .method(method)
        .uri("http://localhost/namespace.json")
        .header("authorization", "Basic unchanged")
        .body(Buffer::new())
        .unwrap();
    (client.fetch(request).await, polls)
}

async fn fetch(
    method: &str,
    status: StatusCode,
    bytes: &[u8],
) -> (Response<HttpBody>, Arc<AtomicUsize>) {
    let (response, polls) = try_fetch(method, status, bytes).await;
    (response.unwrap(), polls)
}

const BLOCKED: &str =
    "<html><body>BlockedTemporarily: Too many requests are received recently</body></html>";

#[tokio::test]
async fn provider_rate_limits_become_non_retryable_rate_limited_errors() {
    for (status, body) in [
        (StatusCode::TOO_MANY_REQUESTS, ""),
        (StatusCode::SERVICE_UNAVAILABLE, BLOCKED),
    ] {
        let (result, _) = try_fetch("GET", status, body.as_bytes()).await;
        let Err(error) = result else {
            panic!("{status} was not rejected");
        };
        assert_eq!(error.kind(), opendal::ErrorKind::RateLimited);
        assert!(!error.is_temporary());
    }
}

#[tokio::test]
async fn throttling_starts_the_cooldown_until_the_provider_answers_normally() {
    static COOLDOWN: Cooldown = Cooldown::new();
    async fn fetch_with(status: StatusCode, body: &'static str) {
        let client = WebDavHttpClient {
            inner: HttpClient::with(CountingFetcher {
                status,
                body,
                calls: Arc::default(),
            }),
            cooldown: &COOLDOWN,
        };
        let request = Request::builder()
            .uri("http://localhost/x")
            .body(Buffer::new())
            .unwrap();
        let _ = client.fetch(request).await;
    }

    fetch_with(StatusCode::SERVICE_UNAVAILABLE, "maintenance").await;
    assert!(COOLDOWN.remaining(Instant::now()).is_none());
    fetch_with(StatusCode::SERVICE_UNAVAILABLE, BLOCKED).await;
    assert!(COOLDOWN.remaining(Instant::now()).is_some());
    fetch_with(StatusCode::OK, "").await;
    assert!(COOLDOWN.remaining(Instant::now()).is_none());
}

#[tokio::test]
async fn plain_service_unavailable_keeps_its_status_and_body() {
    let (mut response, _) = fetch("GET", StatusCode::SERVICE_UNAVAILABLE, b"maintenance").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response.body_mut().to_buffer().await.unwrap().to_vec(),
        b"maintenance"
    );
}

struct CountingFetcher {
    status: StatusCode,
    body: &'static str,
    calls: Arc<AtomicUsize>,
}

impl HttpFetch for CountingFetcher {
    async fn fetch(&self, _: Request<Buffer>) -> opendal::Result<Response<HttpBody>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let bytes = Buffer::from(self.body.as_bytes().to_vec());
        let size = bytes.len() as u64;
        let body = HttpBody::new(futures_util::stream::iter([Ok(bytes)]), Some(size));
        Ok(Response::builder().status(self.status).body(body).unwrap())
    }
}

async fn requests_for_failed_read(
    status: StatusCode,
    body: &'static str,
) -> (usize, opendal::Error) {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = WebDavHttpClient {
        inner: HttpClient::with(CountingFetcher {
            status,
            body,
            calls: calls.clone(),
        }),
        cooldown: Box::leak(Box::new(Cooldown::new())),
    };
    let operator =
        opendal::Operator::new(opendal::services::Webdav::default().endpoint("http://localhost"))
            .unwrap()
            .layer(HttpClientLayer::new(HttpClient::with(client)))
            .layer(
                opendal::layers::RetryLayer::new()
                    .with_min_delay(std::time::Duration::from_millis(1))
                    .with_max_times(3),
            )
            .finish();
    let error = operator.read("namespace.json").await.unwrap_err();
    (calls.load(Ordering::SeqCst), error)
}

#[tokio::test]
async fn a_throttled_read_is_attempted_once_but_other_outages_are_retried() {
    for (status, body) in [
        (StatusCode::TOO_MANY_REQUESTS, ""),
        (StatusCode::SERVICE_UNAVAILABLE, BLOCKED),
    ] {
        let (calls, error) = requests_for_failed_read(status, body).await;
        assert_eq!(calls, 1);
        assert_eq!(error.kind(), opendal::ErrorKind::RateLimited);
    }
    let (calls, _) = requests_for_failed_read(StatusCode::SERVICE_UNAVAILABLE, "maintenance").await;
    assert!(calls > 1);
}

#[tokio::test]
async fn only_missing_ancestor_reads_and_directory_queries_become_not_found() {
    for method in ["GET", "PROPFIND", "PUT", "MKCOL", "DELETE", "HEAD"] {
        let (mut response, _) = fetch(method, StatusCode::CONFLICT, MISSING.as_bytes()).await;
        let expected = if ["GET", "PROPFIND"].contains(&method) {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::CONFLICT
        };
        assert_eq!(response.status(), expected);
        assert_eq!(response.headers()["x-provider"], "unchanged");
        assert_eq!(
            response.body_mut().to_buffer().await.unwrap().to_vec(),
            MISSING.as_bytes()
        );
    }
}

#[tokio::test]
async fn successful_archives_and_other_errors_are_not_buffered_or_rewritten() {
    for status in [
        StatusCode::OK,
        StatusCode::PARTIAL_CONTENT,
        StatusCode::MULTI_STATUS,
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::NOT_FOUND,
        StatusCode::INTERNAL_SERVER_ERROR,
    ] {
        let (mut response, polls) = fetch("GET", status, MISSING.as_bytes()).await;
        assert_eq!(response.status(), status);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert_eq!(
            response.body_mut().to_buffer().await.unwrap().to_vec(),
            MISSING.as_bytes()
        );
    }
    let other = MISSING.replace("AncestorsNotFound", "OperationNotAllowed");
    let (mut response, _) = fetch("GET", StatusCode::CONFLICT, other.as_bytes()).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response.body_mut().to_buffer().await.unwrap().to_vec(),
        other.as_bytes()
    );
}

#[test]
fn exception_matching_is_namespace_aware_and_fails_closed() {
    let another_prefix = MISSING
        .replace("xmlns:s=", "xmlns:provider=")
        .replace("<s:", "<provider:")
        .replace("</s:", "</provider:");
    let default_namespace = r#"<error xmlns="DAV:"><exception xmlns="http://ns.jianguoyun.com">AncestorsNotFound</exception></error>"#;
    for xml in [MISSING, &another_prefix, default_namespace] {
        assert!(is_jianguoyun_missing_ancestors(xml.as_bytes()));
    }
    for xml in [
        MISSING.replace("http://ns.jianguoyun.com", "http://other.example"),
        MISSING.replace("DAV:", "other:"),
        MISSING.replace("AncestorsNotFound", "AncestorsNotFoundExtra"),
        MISSING.replace("s:exception", "s:message"),
        MISSING
            .replace("<s:exception>", "<s:wrapper><s:exception>")
            .replace("</s:exception>", "</s:exception></s:wrapper>"),
        MISSING.replace("</d:error>", ""),
        MISSING.replace("</d:error>", "</wrong>"),
        format!("{MISSING}{MISSING}"),
        format!("{MISSING}trailing"),
        MISSING.replace(
            "</d:error>",
            "<s:exception>AncestorsNotFound</s:exception></d:error>",
        ),
        MISSING.replace("</d:error>", "<s:exception/></d:error>"),
        format!("{}{}", " ".repeat(MAX_ERROR_XML_SIZE), MISSING),
        "not XML: AncestorsNotFound".into(),
    ] {
        assert!(
            !is_jianguoyun_missing_ancestors(xml.as_bytes()),
            "unexpected match: {xml}"
        );
    }
}
