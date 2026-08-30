mod host_config;

use std::{
    collections::BTreeMap,
    convert::Infallible,
    net::Ipv4Addr,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tauri::{AppHandle, Manager};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa::ToSchema;

use host_config::HostConfigStore;

#[derive(Clone)]
pub struct HttpHostState {
    app: AppHandle,
    base_url: Arc<str>,
    api_token: Arc<RwLock<String>>,
    events: Arc<Mutex<HostEventHub>>,
}

impl HttpHostState {
    pub fn app(&self) -> &AppHandle {
        &self.app
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn api_token(&self) -> String {
        self.api_token
            .read()
            .expect("HTTP API token lock poisoned")
            .clone()
    }

    pub fn regenerate_api_token(&self) -> Result<String, ApiError> {
        let mut store = HostConfigStore::load(rgsm_core::app_dirs::get_app_data_dir())
            .map_err(ApiError::from_display)?;
        let token = store.regenerate_token().map_err(ApiError::from_display)?;
        *self
            .api_token
            .write()
            .expect("HTTP API token lock poisoned") = token.clone();
        self.events
            .lock()
            .expect("host event subscribers poisoned")
            .subscribers
            .clear();
        Ok(token)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HostEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Default)]
struct HostEventHub {
    subscribers: Vec<mpsc::Sender<HostEvent>>,
    latest: BTreeMap<String, HostEvent>,
}

impl HostEventHub {
    fn publish(&mut self, event: HostEvent) {
        if is_stateful_event(&event.event_type) {
            self.latest.insert(event.event_type.clone(), event.clone());
        }
        self.subscribers
            .retain(|subscriber| subscriber.try_send(event.clone()).is_ok());
    }

    fn subscribe(&mut self, capacity: usize) -> mpsc::Receiver<HostEvent> {
        let (sender, receiver) = mpsc::channel(capacity);
        for event in self.latest.values() {
            sender
                .try_send(event.clone())
                .expect("latest host events fit in a new subscriber queue");
        }
        self.subscribers.push(sender);
        receiver
    }
}

fn is_stateful_event(event_type: &str) -> bool {
    event_type == "cloud-sync-status"
}

pub fn emit<T: Serialize>(app: &AppHandle, event_type: &str, payload: &T) {
    let Some(state) = app.try_state::<HttpHostState>() else {
        return;
    };
    let Ok(payload) = serde_json::to_value(payload) else {
        log::warn!(target: "rgsm::http", "Failed to serialize host event {event_type}");
        return;
    };
    let event = HostEvent {
        event_type: event_type.to_string(),
        payload,
    };
    let mut hub = state
        .events
        .lock()
        .expect("host event subscribers poisoned");
    hub.publish(event);
}

#[utoipa::path(
    get,
    path = "/api/v1/events",
    operation_id = "streamEvents",
    responses(
        (status = 200, description = "Server-sent host event stream", body = HostEvent, content_type = "text/event-stream"),
        (status = 401, body = ApiError)
    )
)]
pub async fn stream_events(
    State(state): State<HttpHostState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let receiver = state
        .events
        .lock()
        .expect("host event subscribers poisoned")
        .subscribe(128);
    let stream = ReceiverStream::new(receiver).map(|event| {
        Ok(Event::default()
            .json_data(event)
            .expect("host event serializes"))
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    InvalidRequest,
    Unauthorized,
    NotFound,
    Conflict,
    Unavailable,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: ApiErrorCode::InvalidRequest,
            message: message.into(),
            details: None,
        }
    }

    fn unauthorized() -> Self {
        Self {
            code: ApiErrorCode::Unauthorized,
            message: "Unauthorized".to_string(),
            details: None,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: ApiErrorCode::Conflict,
            message: message.into(),
            details: None,
        }
    }

    pub fn from_command(error: impl std::fmt::Display) -> Self {
        Self {
            code: ApiErrorCode::InvalidRequest,
            message: error.to_string(),
            details: None,
        }
    }

    pub fn from_display(error: impl std::fmt::Display) -> Self {
        Self {
            code: ApiErrorCode::Internal,
            message: error.to_string(),
            details: None,
        }
    }

    pub fn from_serializable<T>(error: T) -> Self
    where
        T: std::fmt::Display + Serialize,
    {
        Self {
            code: ApiErrorCode::InvalidRequest,
            message: error.to_string(),
            details: serde_json::to_value(error).ok(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.code {
            ApiErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
            ApiErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiErrorCode::NotFound => StatusCode::NOT_FOUND,
            ApiErrorCode::Conflict => StatusCode::CONFLICT,
            ApiErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            ApiErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self)).into_response()
    }
}

pub struct HttpHost {
    pub base_url: String,
}

pub fn prepare_configuration() -> anyhow::Result<()> {
    HostConfigStore::prepare(rgsm_core::app_dirs::get_app_data_dir())?;
    Ok(())
}

pub async fn start(app: AppHandle) -> anyhow::Result<HttpHost> {
    let app_data_dir = rgsm_core::app_dirs::get_app_data_dir();
    let mut config_store = HostConfigStore::load(app_data_dir)?;
    let configured_port = config_store.config().port;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, configured_port))
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "Cannot bind RGSM HTTP Host to 127.0.0.1:{configured_port}: {error}. Host configuration: {}",
                config_store.path().display()
            )
        })?;
    let port = listener.local_addr()?.port();
    config_store.set_bound_port(port)?;

    let api_token = config_store.config().api_token.clone();
    let events = Arc::new(Mutex::new(HostEventHub::default()));
    let base_url = format!("http://127.0.0.1:{port}");
    let state = HttpHostState {
        app: app.clone(),
        base_url: Arc::from(base_url.clone()),
        api_token: Arc::new(RwLock::new(api_token.clone())),
        events,
    };
    app.manage(state.clone());

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("http://localhost:5173"),
            HeaderValue::from_static("http://tauri.localhost"),
            HeaderValue::from_static("tauri://localhost"),
        ]))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    let router = crate::commands::http_commands::router()
        .route("/api/v1/events", get(stream_events))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_authentication,
        ))
        .layer(cors)
        .with_state(state);

    tauri::async_runtime::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            log::error!(target: "rgsm::http", "HTTP Host stopped: {error}");
        }
    });

    Ok(HttpHost { base_url })
}

fn is_allowed_browser_origin(origin: &HeaderValue) -> bool {
    matches!(
        origin.as_bytes(),
        b"http://localhost:5173" | b"http://tauri.localhost" | b"tauri://localhost"
    )
}

fn accepts_json_content_type(request: &Request) -> bool {
    request
        .headers()
        .get(header::CONTENT_TYPE)
        .is_none_or(|content_type| content_type.as_bytes().starts_with(b"application/json"))
}

fn has_valid_bearer(request: &Request, expected: &str) -> bool {
    let Some(provided) = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    provided.len() == expected.len() && bool::from(provided.as_bytes().ct_eq(expected.as_bytes()))
}

async fn require_authentication(
    State(state): State<HttpHostState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if let Some(origin) = request.headers().get(header::ORIGIN)
        && !is_allowed_browser_origin(origin)
    {
        return Err(ApiError::unauthorized());
    }

    if !accepts_json_content_type(&request) {
        return Err(ApiError::invalid_request(
            "Only application/json request bodies are accepted",
        ));
    }

    let is_valid = {
        let expected = state
            .api_token
            .read()
            .expect("HTTP API token lock poisoned");
        has_valid_bearer(&request, &expected)
    };
    if !is_valid {
        return Err(ApiError::unauthorized());
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_untrusted_browser_origins() {
        assert!(is_allowed_browser_origin(&HeaderValue::from_static(
            "http://localhost:5173"
        )));
        assert!(!is_allowed_browser_origin(&HeaderValue::from_static(
            "https://attacker.example"
        )));
    }

    #[test]
    fn api_error_omits_empty_details() {
        let json = serde_json::to_value(ApiError::unauthorized()).unwrap();
        assert_eq!(json["code"], "unauthorized");
        assert!(json.get("details").is_none());
    }

    #[test]
    fn requires_exact_bearer_token_and_json_content_type() {
        let valid = Request::builder()
            .header(header::AUTHORIZATION, "Bearer secret")
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(has_valid_bearer(&valid, "secret"));
        assert!(accepts_json_content_type(&valid));

        let invalid_token = Request::builder()
            .header(header::AUTHORIZATION, "Bearer other")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(!has_valid_bearer(&invalid_token, "secret"));

        let invalid_content_type = Request::builder()
            .header(header::AUTHORIZATION, "Bearer secret")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(!accepts_json_content_type(&invalid_content_type));
    }

    #[test]
    fn reconnect_replays_latest_event_after_slow_subscriber_is_dropped() {
        let mut hub = HostEventHub::default();
        let mut slow = hub.subscribe(1);
        hub.publish(HostEvent {
            event_type: "cloud-sync-status".into(),
            payload: serde_json::json!({ "sequence": 1 }),
        });
        hub.publish(HostEvent {
            event_type: "cloud-sync-status".into(),
            payload: serde_json::json!({ "sequence": 2 }),
        });

        assert_eq!(slow.try_recv().unwrap().payload["sequence"], 1);
        assert!(slow.try_recv().is_err());

        let mut reconnected = hub.subscribe(1);
        assert_eq!(reconnected.try_recv().unwrap().payload["sequence"], 2);
    }

    #[test]
    fn reconnect_replays_only_stateful_events() {
        let mut hub = HostEventHub::default();
        hub.publish(HostEvent {
            event_type: "notification".into(),
            payload: serde_json::json!({ "message": "one shot" }),
        });
        hub.publish(HostEvent {
            event_type: "cloud-sync-status".into(),
            payload: serde_json::json!({ "sequence": 1 }),
        });

        let mut reconnected = hub.subscribe(2);
        assert_eq!(
            reconnected.try_recv().unwrap().event_type,
            "cloud-sync-status"
        );
        assert!(reconnected.try_recv().is_err());
    }
}
