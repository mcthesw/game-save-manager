//! Provider response compatibility at the WebDAV HTTP boundary.

use std::time::Instant;

use http::{Method, Request, Response, StatusCode};
use opendal::Buffer;
use opendal::layers::HttpClientLayer;
use opendal::raw::{HttpBody, HttpClient, HttpFetch};
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;

use super::rate_limit::Cooldown;

const JIANGUOYUN_NAMESPACE: &[u8] = b"http://ns.jianguoyun.com";
const JIANGUOYUN_BLOCKED: &str = "BlockedTemporarily";
const MAX_ERROR_XML_SIZE: usize = 16 * 1024;

pub(super) fn compatibility_layer() -> opendal::Result<HttpClientLayer> {
    Ok(HttpClientLayer::new(HttpClient::with(WebDavHttpClient {
        inner: HttpClient::new()?,
        cooldown: &super::rate_limit::PROVIDER,
    })))
}

struct WebDavHttpClient {
    inner: HttpClient,
    cooldown: &'static Cooldown,
}

impl HttpFetch for WebDavHttpClient {
    async fn fetch(&self, request: Request<Buffer>) -> opendal::Result<Response<HttpBody>> {
        let result = self.normalize(request).await;
        match &result {
            Err(error) if error.kind() == opendal::ErrorKind::RateLimited => {
                self.cooldown.trip(Instant::now());
            }
            Ok(_) => self.cooldown.clear(),
            Err(_) => {}
        }
        result
    }
}

impl WebDavHttpClient {
    async fn normalize(&self, request: Request<Buffer>) -> opendal::Result<Response<HttpBody>> {
        let checks_existence = request.method() == Method::GET || request.method() == "PROPFIND";
        let response = self.inner.fetch(request).await?;
        let status = response.status();
        // OpenDAL maps 429 to a generic error and retries 503, which keeps
        // hammering a provider that is already throttling us.
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(rate_limited(status));
        }
        let blocked_check = status == StatusCode::SERVICE_UNAVAILABLE;
        let missing_check = checks_existence && status == StatusCode::CONFLICT;
        if !blocked_check && !missing_check {
            return Ok(response);
        }

        let (mut parts, mut body) = response.into_parts();
        let buffer = body.to_buffer().await?;
        if blocked_check {
            // A plain 503 is a transient outage worth retrying; only
            // Jianguoyun's explicit block means we are over its request quota.
            if buffer.len() <= MAX_ERROR_XML_SIZE
                && String::from_utf8_lossy(&buffer.to_vec()).contains(JIANGUOYUN_BLOCKED)
            {
                return Err(rate_limited(status));
            }
        } else if buffer.len() <= MAX_ERROR_XML_SIZE
            && is_jianguoyun_missing_ancestors(&buffer.to_vec())
        {
            // Jianguoyun returns 409/AncestorsNotFound when the configured root is
            // absent, including for GET and PROPFIND. OpenDAL 0.57 otherwise treats
            // it as Unexpected, blocking both read-only inspection and the writer's
            // parent-directory creation. Normalize only this namespaced error to
            // 404; write conflicts and authentication failures must retain their
            // original meaning. Successful archive bodies stay streamed.
            // See https://github.com/mcthesw/game-save-manager/issues/587.
            parts.status = StatusCode::NOT_FOUND;
        }
        let size = buffer.len() as u64;
        let body = HttpBody::new(futures_util::stream::iter([Ok(buffer)]), Some(size));
        Ok(Response::from_parts(parts, body))
    }
}

/// Not marked temporary, so the retry layer gives up immediately.
fn rate_limited(status: StatusCode) -> opendal::Error {
    opendal::Error::new(
        opendal::ErrorKind::RateLimited,
        "the WebDAV server is limiting the request rate",
    )
    .with_context("status", status.as_str())
}

fn is_jianguoyun_missing_ancestors(bytes: &[u8]) -> bool {
    if bytes.len() > MAX_ERROR_XML_SIZE {
        return false;
    }
    let mut reader = NsReader::from_reader(bytes);
    let mut depth = 0;
    let mut root_seen = false;
    let mut matched = false;
    loop {
        let Ok((namespace, event)) = reader.read_resolved_event() else {
            return false;
        };
        match event {
            Event::Start(element) => {
                if depth == 0 {
                    if root_seen
                        || element.local_name().as_ref() != b"error"
                        || !matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == b"DAV:")
                    {
                        return false;
                    }
                    root_seen = true;
                } else if depth == 1
                    && element.local_name().as_ref() == b"exception"
                    && matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == JIANGUOYUN_NAMESPACE)
                {
                    let Ok(text) = reader.read_text(element.name()) else {
                        return false;
                    };
                    if matched || text.trim() != "AncestorsNotFound" {
                        return false;
                    }
                    matched = true;
                    continue;
                }
                depth += 1;
            }
            Event::End(_) => depth -= 1,
            Event::Empty(element)
                if depth == 0
                    || (depth == 1
                        && element.local_name().as_ref() == b"exception"
                        && matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == JIANGUOYUN_NAMESPACE)) =>
            {
                return false;
            }
            Event::Text(text) if depth == 0 && !text.iter().all(u8::is_ascii_whitespace) => {
                return false;
            }
            Event::DocType(_) => return false,
            Event::Eof => return root_seen && depth == 0 && matched,
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "webdav_tests.rs"]
mod tests;
