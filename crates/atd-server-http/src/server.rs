//! `Server` — the HTTP listener and accept loop.
//!
//! SP-streamable-http §4.1 + §4.5: the listener exposes a single `POST
//! /mcp` route by default, but `ServerBuilder::build` returns the
//! `axum::Router` so adopters can extend it with their own routes
//! (Celia's `/chat/stream`, healthkit's bulk-export progress, etc.)
//! before serving. The bound `Server` carries the shutdown handle and
//! the `local_addr` so tests can dial in.

use std::net::SocketAddr;
use std::sync::Arc;

use atd_runtime::dispatch::ServerState;
use atd_runtime::registry::Registry;
use atd_runtime::secrets::BearerIdentity;
use axum::Json;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Router, serve};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::bearer::{BearerOutcome, resolve_bearer};
use crate::config::HttpServerConfig;
use crate::error::HttpServerError;
use crate::mcp::{
    JsonRpcRequest, error_response, error_response_with_headers, handle_initialize,
    handle_initialized_notification, handle_tools_call, handle_tools_list,
};
use crate::origin::origin_allowed;

/// Per-request state shared with each axum handler. Holds the dispatch
/// state, the operator policy fields the route handlers read directly
/// (origin extras, bearer requirement) and the route-handler-local copy
/// of the operator allow-list pulled out of the shared config so
/// handlers don't have to chain `.config.shared.*` paths at every call
/// site.
#[derive(Clone)]
pub(crate) struct HttpAppState {
    pub state: Arc<ServerState>,
    pub extra_origins: Arc<Vec<String>>,
    pub require_bearer: bool,
    pub server_version: Arc<String>,
}

pub struct ServerBuilder {
    registry: Registry,
    config: HttpServerConfig,
    tier_policy: atd_runtime::TierPolicy,
    middleware: Vec<Arc<dyn atd_runtime::Middleware>>,
}

impl ServerBuilder {
    pub fn config(mut self, cfg: HttpServerConfig) -> Self {
        self.config = cfg;
        self
    }

    pub fn tier_policy(mut self, policy: atd_runtime::TierPolicy) -> Self {
        self.tier_policy = policy;
        self
    }

    pub fn middleware(mut self, middleware: Vec<Arc<dyn atd_runtime::Middleware>>) -> Self {
        self.middleware = middleware;
        self
    }

    /// Finalise the builder into a `(Router, Server)` pair. Adopters who
    /// want to mount additional routes call `.route(...)` on the
    /// returned `Router` before invoking `Server::serve`. The default
    /// router only carries `POST /mcp` and its CORS preflight.
    pub fn build(self) -> (Router, Server) {
        let HttpServerConfig {
            listen,
            extra_origins,
            require_bearer,
            max_body_bytes,
            shared,
        } = self.config;

        let server_version = Arc::new(shared.server_version.clone());

        // SP-streamable-http §4.3: HTTP and UDS share the *same*
        // `ServerState` shape. The HTTP listener owns the `Arc` directly
        // here; tests that drive UDS + HTTP in one process build one
        // `ServerState` and pass clones to both transports.
        let server_state = Arc::new(ServerState {
            registry: self.registry,
            config: Arc::try_unwrap(shared).unwrap_or_else(|arc| {
                // If the `Arc` was already cloned elsewhere, fall back to a
                // by-value copy via the public fields. SharedServerConfig
                // does not impl Clone (audit_sink / token_broker are
                // trait objects), so we rebuild it field by field.
                atd_runtime::dispatch::SharedServerConfig {
                    cwd: arc.cwd.clone(),
                    max_output_bytes: arc.max_output_bytes,
                    default_call_timeout_ms: arc.default_call_timeout_ms,
                    granted_capabilities: arc.granted_capabilities.clone(),
                    audit_sink: arc.audit_sink.clone(),
                    server_version: arc.server_version.clone(),
                    token_broker: arc.token_broker.clone(),
                    max_ucan_chain_depth: arc.max_ucan_chain_depth,
                    ucan_revocation_store: arc.ucan_revocation_store.clone(),
                }
            }),
            tier_policy: self.tier_policy,
            middleware: self.middleware,
        });

        let app_state = HttpAppState {
            state: server_state,
            extra_origins: Arc::new(extra_origins),
            require_bearer,
            server_version,
        };

        let router = Router::new()
            .route("/mcp", post(handle_mcp_post))
            // Body-size cap — requests larger than this short-circuit
            // with HTTP 413. SP-streamable-http §5.6.
            .layer(DefaultBodyLimit::max(max_body_bytes))
            .with_state(app_state);

        let server = Server {
            listen,
            shutdown_tx: None,
            local_addr: None,
        };
        (router, server)
    }
}

pub struct Server {
    listen: SocketAddr,
    /// Sender side of the shutdown oneshot. `Some` between `serve` start
    /// and shutdown; `None` before / after.
    shutdown_tx: Option<oneshot::Sender<()>>,
    local_addr: Option<SocketAddr>,
}

impl Server {
    /// Begin a builder chain for an HTTP server backed by `registry`.
    pub fn builder(registry: Registry) -> ServerBuilder {
        ServerBuilder {
            registry,
            config: HttpServerConfig::default(),
            tier_policy: atd_runtime::TierPolicy::defaults(),
            middleware: Vec::new(),
        }
    }

    /// Address the server is bound to. `None` until the listener has
    /// successfully bound (i.e. after `serve` has set it). Tests use
    /// this to discover the kernel-chosen port when `listen` was
    /// `127.0.0.1:0`.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    /// Trigger a graceful shutdown. Returns `false` if the server was
    /// never started or has already been signalled.
    pub fn shutdown(&mut self) -> bool {
        match self.shutdown_tx.take() {
            Some(tx) => tx.send(()).is_ok(),
            None => false,
        }
    }

    /// Bind + serve. Returns when the listener stops accepting (typically
    /// triggered by `Server::shutdown` or a fatal accept error).
    pub async fn serve(mut self, router: Router) -> Result<(), HttpServerError> {
        let listener =
            TcpListener::bind(self.listen)
                .await
                .map_err(|source| HttpServerError::Bind {
                    addr: self.listen,
                    source,
                })?;
        let local = listener
            .local_addr()
            .map_err(|source| HttpServerError::Bind {
                addr: self.listen,
                source,
            })?;
        self.local_addr = Some(local);

        let (tx, rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(tx);

        serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await?;
        Ok(())
    }

    /// Bind only — returns the bound `TcpListener` + the resolved local
    /// address. Tests use this to learn the kernel-chosen port before
    /// driving `serve` on a separate task.
    pub async fn bind(&mut self) -> Result<TcpListener, HttpServerError> {
        let listener =
            TcpListener::bind(self.listen)
                .await
                .map_err(|source| HttpServerError::Bind {
                    addr: self.listen,
                    source,
                })?;
        let local = listener
            .local_addr()
            .map_err(|source| HttpServerError::Bind {
                addr: self.listen,
                source,
            })?;
        self.local_addr = Some(local);
        Ok(listener)
    }

    /// Serve on a pre-bound listener (paired with `bind`). Useful for
    /// tests that need the local address before the serve future starts.
    pub async fn serve_with_listener(
        mut self,
        listener: TcpListener,
        router: Router,
    ) -> Result<(), HttpServerError> {
        let (tx, rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(tx);
        serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await?;
        Ok(())
    }
}

/// `POST /mcp` axum handler. Implements the three-step pipeline:
/// origin gate → bearer resolution → method dispatch. SP-streamable-http
/// §4.6 / §4.4 / §4.2.
async fn handle_mcp_post(
    State(app): State<HttpAppState>,
    headers: HeaderMap,
    body: Result<Json<JsonRpcRequest>, axum::extract::rejection::JsonRejection>,
) -> axum::response::Response {
    // Step 0: parse the JSON-RPC body. axum returns a typed rejection
    // for malformed JSON / wrong content-type / body too large; we
    // surface those as JSON-RPC invalid-request / parse errors per
    // SP-streamable-http §5.6.
    let Json(req) = match body {
        Ok(j) => j,
        Err(rej) => {
            // Body-size rejection → 413 + -32600; everything else → 400
            // + -32700 (parse) for malformed JSON.
            let (status, code, msg) = body_rejection_status(&rej);
            return error_response(status, None, code, msg);
        }
    };

    // Step 1: Origin gate.
    if !origin_allowed(&headers, &app.extra_origins) {
        return error_response(StatusCode::FORBIDDEN, req.id, -32001, "origin not allowed");
    }

    // Step 2: Bearer resolution.
    // SP-token-broker-phase2 §4.4: per-outcome HTTP status (401/400/500/501/503)
    // + headers (WWW-Authenticate, Retry-After) come from BearerOutcome's
    // accessor methods. The JSON-RPC envelope's `code` stays at -32002
    // (auth) for all non-admitted outcomes; the HTTP-layer distinctions
    // are the load-bearing signal for adopter clients.
    let outcome = resolve_bearer(
        &headers,
        app.state.config.token_broker.as_ref(),
        app.require_bearer,
    )
    .await;

    let identity: Option<BearerIdentity> = match outcome {
        BearerOutcome::Anonymous => None,
        BearerOutcome::Validated(id) => Some(id),
        rejection => {
            let status = rejection.http_status();
            let message = rejection
                .rejection_message()
                .unwrap_or_else(|| "bearer auth failed".into());
            let mut headers: Vec<(&str, String)> = Vec::new();
            if let Some(www) = rejection.www_authenticate() {
                headers.push(("WWW-Authenticate", www.to_string()));
            }
            if let Some(retry) = rejection.retry_after() {
                headers.push(("Retry-After", retry.to_string()));
            }
            return error_response_with_headers(status, req.id, -32002, message, &headers);
        }
    };

    // Step 3: Method dispatch.
    match req.method.as_str() {
        "initialize" => handle_initialize(
            req.id,
            &app.server_version,
            app.state.config.token_broker.as_ref(),
        ),
        "notifications/initialized" => handle_initialized_notification(req.id),
        "tools/list" => handle_tools_list(req.id, &app.state),
        "tools/call" => handle_tools_call(req.id, &app.state, identity.as_ref(), req.params).await,
        other => error_response(
            StatusCode::OK,
            req.id,
            -32601,
            format!("method not found: {other}"),
        ),
    }
}

/// Map axum's `JsonRejection` into `(status, jsonrpc_code, message)`.
fn body_rejection_status(
    rej: &axum::extract::rejection::JsonRejection,
) -> (StatusCode, i32, String) {
    use axum::extract::rejection::JsonRejection;
    match rej {
        JsonRejection::JsonDataError(e) => (StatusCode::BAD_REQUEST, -32600, e.to_string()),
        JsonRejection::JsonSyntaxError(e) => (StatusCode::BAD_REQUEST, -32700, e.to_string()),
        JsonRejection::MissingJsonContentType(_) => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            -32600,
            "expected application/json".into(),
        ),
        JsonRejection::BytesRejection(_) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            -32600,
            "body too large".into(),
        ),
        other => (
            StatusCode::BAD_REQUEST,
            -32600,
            format!("invalid request body: {other}"),
        ),
    }
}

#[allow(dead_code)]
fn body_too_large_response(id: Option<Value>) -> axum::response::Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32600, "message": "body too large"},
        })),
    )
        .into_response()
}
