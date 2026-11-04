//! Provider response compatibility at the WebDAV HTTP boundary.

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures_util::{Stream, StreamExt};
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
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// WebDAV time limits live at the HTTP boundary rather than in OpenDAL's
/// timeout layer: that layer gives a whole one-shot upload the same fixed
/// budget as a metadata call, so it cuts off any large archive on a slow link.
#[derive(Clone, Copy)]
pub(super) struct Limits {
    /// Time for a request without a real body, and the longest silence tolerated
    /// while a response is being received.
    pub(super) request: Duration,
    /// Slowest upload speed, in bytes per second, that still counts as progress.
    pub(super) upload_rate: u64,
}

impl Limits {
    const DEFAULT: Self = Self {
        request: Duration::from_secs(30),
        upload_rate: 256 * 1024,
    };

    fn request_time(&self, body_bytes: usize) -> Duration {
        self.request + Duration::from_secs(body_bytes as u64 / self.upload_rate)
    }
}

pub(super) fn compatibility_layer() -> opendal::Result<HttpClientLayer> {
    compatibility_layer_with(Limits::DEFAULT)
}

pub(super) fn compatibility_layer_with(limits: Limits) -> opendal::Result<HttpClientLayer> {
    let client = reqwest_opendal::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|error| {
            opendal::Error::new(
                opendal::ErrorKind::Unexpected,
                "cannot build the HTTP client",
            )
            .set_source(error)
        })?;
    Ok(HttpClientLayer::new(HttpClient::with(WebDavHttpClient {
        inner: HttpClient::with(client),
        cooldown: &super::rate_limit::PROVIDER,
        limits,
    })))
}

struct WebDavHttpClient {
    inner: HttpClient,
    cooldown: &'static Cooldown,
    limits: Limits,
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
        // A PUT answers only after its whole body is sent, so the allowance
        // grows with the archive; the retry layer replays a timed-out request.
        let allowed = self.limits.request_time(request.body().len());
        let response = tokio::time::timeout(allowed, self.inner.fetch(request))
            .await
            .map_err(|_| timed_out("the WebDAV server did not answer in time", allowed))??;
        let idle = self.limits.request;
        let response =
            response.map(|body| body.map_inner(|stream| Box::new(StallGuard::new(stream, idle))));
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

fn timed_out(message: &'static str, limit: Duration) -> opendal::Error {
    opendal::Error::new(opendal::ErrorKind::Unexpected, message)
        .with_context("seconds", limit.as_secs().to_string())
        .set_temporary()
}

type BodyStream = Box<dyn Stream<Item = opendal::Result<Buffer>> + Send + Sync + Unpin>;

/// Fails a download that stops delivering data, however large it is. The clock
/// starts when the reader asks for the next chunk, so a slow consumer is not
/// mistaken for a stalled server.
struct StallGuard {
    inner: BodyStream,
    idle: Pin<Box<tokio::time::Sleep>>,
    limit: Duration,
    waiting: bool,
}

impl StallGuard {
    fn new(inner: BodyStream, limit: Duration) -> Self {
        Self {
            inner,
            idle: Box::pin(tokio::time::sleep(limit)),
            limit,
            waiting: false,
        }
    }
}

impl Stream for StallGuard {
    type Item = opendal::Result<Buffer>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        if !this.waiting {
            this.waiting = true;
            this.idle
                .as_mut()
                .reset(tokio::time::Instant::now() + this.limit);
        }
        if let Poll::Ready(item) = this.inner.poll_next_unpin(cx) {
            this.waiting = false;
            return Poll::Ready(item);
        }
        match this.idle.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(Some(Err(timed_out(
                "the WebDAV server stopped sending data",
                this.limit,
            )))),
            Poll::Pending => Poll::Pending,
        }
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
