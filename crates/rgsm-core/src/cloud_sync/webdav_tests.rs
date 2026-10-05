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

async fn fetch(
    method: &str,
    status: StatusCode,
    bytes: &[u8],
) -> (Response<HttpBody>, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    let client = WebDavHttpClient {
        inner: HttpClient::with(Fetcher {
            status,
            bytes: bytes.to_vec(),
            polls: polls.clone(),
        }),
    };
    let request = Request::builder()
        .method(method)
        .uri("http://localhost/namespace.json")
        .header("authorization", "Basic unchanged")
        .body(Buffer::new())
        .unwrap();
    (client.fetch(request).await.unwrap(), polls)
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
        StatusCode::TOO_MANY_REQUESTS,
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
