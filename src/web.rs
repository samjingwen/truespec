use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::{
    anchor::{Anchor, Location, Status, Target},
    inbox::{LocalInbox, PROTOCOL_VERSION},
    markdown::{Heading, Rendered, SourceLine, render, safe_link},
    review::{
        AgentReply, BatchItem, Comment, DraftRef, Problem, Session, Store, Submission,
        SubmittedSource, UnsupportedDatabaseVersion, conflict, invalid,
    },
    service::{
        AttachRequest, AttachResponse, DeferRequest, DeliveryRequest, Health, OwnerLock,
        PollRequest, Registration, RespondRequest, ServiceConfig, ServicePaths,
        WorkspaceRegistration, authorized,
    },
    service_registry::{DuplicateWorkspaceIdentity, ProjectContext, ProjectRegistry},
    source::{Document, DocumentSource, Snapshot},
};

const BROWSER_BODY_LIMIT: usize = 256 * 1024;
const CONNECTOR_REPLY_BODY_LIMIT: usize = 7 * 1024 * 1024;

#[derive(Clone)]
struct App {
    store: Store,
    source: Arc<dyn DocumentSource>,
    workspace: PathBuf,
    source_name: &'static str,
}

#[derive(Clone)]
struct SharedApp {
    registry: ProjectRegistry,
    config: ServiceConfig,
    registration: Arc<Mutex<Registration>>,
    paths: ServicePaths,
    shutdown: Arc<Notify>,
    shutdown_requested: Arc<AtomicBool>,
}

#[derive(Clone)]
struct BrowserAccess {
    local_host: String,
    local_origin: String,
    tailnet_host: Option<String>,
    tailnet_origin: Option<String>,
}

pub async fn serve_shared(
    listener: TcpListener,
    registry: ProjectRegistry,
    config: ServiceConfig,
    registration: Registration,
    paths: ServicePaths,
    _owner: OwnerLock,
) -> Result<()> {
    let address = listener.local_addr()?.to_string();
    let origin = format!("http://{address}");
    let browser_access = BrowserAccess::new(address.clone(), origin, config.tailnet())?;
    let shutdown = Arc::new(Notify::new());
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let state = SharedApp {
        registry,
        config,
        registration: Arc::new(Mutex::new(registration)),
        paths,
        shutdown: shutdown.clone(),
        shutdown_requested: shutdown_requested.clone(),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/w/{workspace}", get(index))
        .route("/w/{workspace}/", get(index))
        .route("/app.js", get(app_js))
        .route("/app.css", get(app_css))
        .route("/targets.js", get(targets_js))
        .route("/favicon.svg", get(favicon))
        .route(
            "/api/workspaces/{workspace}/bootstrap",
            get(shared_bootstrap),
        )
        .route(
            "/api/workspaces/{workspace}/documents",
            get(shared_documents),
        )
        .route(
            "/api/workspaces/{workspace}/documents/{id}",
            get(shared_document),
        )
        .route(
            "/api/workspaces/{workspace}/reviews/{review}",
            get(shared_board),
        )
        .route(
            "/api/workspaces/{workspace}/reviews/{review}/comments",
            post(shared_add_comment),
        )
        .route(
            "/api/workspaces/{workspace}/reviews/{review}/general",
            post(shared_add_general_comment),
        )
        .route(
            "/api/workspaces/{workspace}/reviews/{review}/comments/{id}",
            post(shared_change_comment),
        )
        .route(
            "/api/workspaces/{workspace}/reviews/{review}/submit",
            post(shared_submit),
        )
        .route(
            "/api/workspaces/{workspace}/reviews/{review}/end",
            post(shared_end_session),
        )
        .route(
            "/api/workspaces/{workspace}/reviews/{review}/new",
            post(shared_new_session),
        )
        .route("/__control/health", get(control_health))
        .route(
            "/__control/workspaces/register",
            post(control_register_workspace),
        )
        .route(
            "/__control/workspaces/resolve",
            post(control_resolve_workspace),
        )
        .route(
            "/__control/connectors/attach",
            post(control_connector_attach),
        )
        .route("/__control/connectors/poll", post(control_connector_poll))
        .route(
            "/__control/connectors/delivered",
            post(control_connector_delivered),
        )
        .route("/__control/connectors/defer", post(control_connector_defer))
        .route(
            "/__control/connectors/respond",
            post(control_connector_respond)
                .layer(DefaultBodyLimit::max(CONNECTOR_REPLY_BODY_LIMIT)),
        )
        .route("/__control/shutdown", post(control_shutdown))
        .layer(DefaultBodyLimit::max(BROWSER_BODY_LIMIT))
        .layer(middleware::from_fn_with_state(
            browser_access,
            trusted_browser,
        ))
        .with_state(state);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown.notified() => {}
                _ = shutdown_signal() => {}
            }
            shutdown_requested.store(true, Ordering::Release);
        })
        .await?;
    Ok(())
}

async fn index() -> impl IntoResponse {
    asset(
        "text/html; charset=utf-8",
        include_str!("../web/index.html"),
    )
}

async fn app_js() -> impl IntoResponse {
    asset(
        "text/javascript; charset=utf-8",
        include_str!("../web/app.js"),
    )
}

async fn app_css() -> impl IntoResponse {
    asset("text/css; charset=utf-8", include_str!("../web/app.css"))
}

async fn targets_js() -> impl IntoResponse {
    asset(
        "text/javascript; charset=utf-8",
        include_str!("../web/targets.js"),
    )
}

async fn favicon() -> impl IntoResponse {
    asset("image/svg+xml", include_str!("../web/favicon.svg"))
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = terminate.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

impl BrowserAccess {
    fn new(
        local_host: String,
        local_origin: String,
        tailnet: &crate::service::TailnetConfig,
    ) -> Result<Self> {
        let (tailnet_host, tailnet_origin) = if tailnet.enabled {
            let origin = tailnet
                .origin
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Tailnet browser origin is missing"))?;
            let url = reqwest::Url::parse(origin)?;
            let host = url
                .host_str()
                .ok_or_else(|| anyhow::anyhow!("Tailnet browser origin has no host"))?;
            let host = match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            };
            (Some(host), Some(origin.to_owned()))
        } else {
            (None, None)
        };
        Ok(Self {
            local_host,
            local_origin,
            tailnet_host,
            tailnet_origin,
        })
    }
}

async fn trusted_browser(
    State(access): State<BrowserAccess>,
    request: Request,
    next: Next,
) -> Response {
    if !browser_request_allowed(
        &access,
        request.method(),
        request.uri().path(),
        request.headers(),
    ) {
        return (StatusCode::FORBIDDEN, "Use a trusted TrueSpec browser URL.").into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(
        "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"
    ));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    response
}

fn browser_request_allowed(
    access: &BrowserAccess,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
) -> bool {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    let safe = matches!(*method, Method::GET | Method::HEAD);
    let control = path.starts_with("/__control/");
    let identified = headers
        .get("tailscale-user-login")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| !value.trim().is_empty());
    let forwarded_host = headers
        .get("x-forwarded-host")
        .and_then(|value| value.to_str().ok());
    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| !value.trim().is_empty());
    let proxied = identified || forwarded_host.is_some() || forwarded_for;
    if proxied {
        let (Some(tailnet_host), Some(tailnet_origin)) = (
            access.tailnet_host.as_deref(),
            access.tailnet_origin.as_deref(),
        ) else {
            return false;
        };
        return !headers.contains_key("tailscale-funnel-request")
            && !control
            && identified
            && forwarded_for
            && host == Some(tailnet_host)
            && forwarded_host == Some(tailnet_host)
            && (safe || origin == Some(tailnet_origin));
    }
    host == Some(access.local_host.as_str())
        && (safe
            || origin == Some(access.local_origin.as_str())
            || (control && headers.contains_key(header::AUTHORIZATION)))
}

fn asset(content_type: &'static str, text: &'static str) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, content_type)], text)
}

struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message, confirmation) = match self.0.downcast_ref::<Problem>() {
            Some(Problem::Invalid(message)) => {
                (StatusCode::UNPROCESSABLE_ENTITY, message.as_str(), None)
            }
            Some(Problem::Missing(message)) => (StatusCode::NOT_FOUND, *message, None),
            Some(Problem::Conflict(message)) => (StatusCode::CONFLICT, message.as_str(), None),
            Some(Problem::Unavailable(message)) => {
                (StatusCode::SERVICE_UNAVAILABLE, *message, None)
            }
            Some(Problem::Unauthorized(message)) => (StatusCode::UNAUTHORIZED, *message, None),
            Some(Problem::Confirm {
                message,
                confirmation,
            }) => (StatusCode::CONFLICT, message.as_str(), Some(confirmation)),
            None => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "TrueSpec could not finish the request. Check service status and try again.",
                None,
            ),
        };
        (
            code,
            Json(serde_json::json!({ "message": message, "confirmation": confirmation })),
        )
            .into_response()
    }
}

async fn run<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|err| ApiError(err.into()))?
        .map_err(ApiError)
}

fn conditional(value: impl Serialize, headers: &HeaderMap) -> Result<Response, ApiError> {
    let bytes = serde_json::to_vec(&value).map_err(|error| ApiError(error.into()))?;
    let etag = format!("\"{:x}\"", Sha256::digest(&bytes));
    let matched = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|tag| tag.to_str().ok())
        .is_some_and(|tags| tags.split(',').map(str::trim).any(|tag| tag == etag));
    let mut response = if matched {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        let mut response = Response::new(Body::from(bytes));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response
    };
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag).expect("ASCII hex"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    Ok(response)
}

fn control_authorized(headers: &HeaderMap, app: &SharedApp) -> Result<(), ApiError> {
    let header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if authorized(header, &app.config) {
        Ok(())
    } else {
        Err(ApiError(
            Problem::Unauthorized("Private service authentication failed.").into(),
        ))
    }
}

async fn control_health(
    State(app): State<SharedApp>,
    headers: HeaderMap,
) -> Result<Json<Health>, ApiError> {
    control_authorized(&headers, &app)?;
    let registry = app.registry.clone();
    let (projects, open_reviews) = run(move || registry.counts()).await?;
    let registration = {
        let mut registration = app
            .registration
            .lock()
            .map_err(|_| ApiError(anyhow::anyhow!("Service registration is unavailable")))?;
        if registration.state == crate::service::RegistrationState::Starting {
            let mut ready = registration.clone();
            ready.state = crate::service::RegistrationState::Ready;
            ready.write(&app.paths).map_err(ApiError)?;
            *registration = ready;
        }
        registration.clone()
    };
    Ok(Json(Health {
        protocol: registration.protocol,
        version: registration.version,
        instance_id: registration.instance_id,
        pid: registration.pid,
        state: "ready".into(),
        projects,
        open_reviews,
    }))
}

#[derive(Deserialize)]
struct RegisterWorkspaceRequest {
    root: PathBuf,
}

async fn control_register_workspace(
    State(app): State<SharedApp>,
    headers: HeaderMap,
    Json(request): Json<RegisterWorkspaceRequest>,
) -> Result<Json<WorkspaceRegistration>, ApiError> {
    control_authorized(&headers, &app)?;
    let registry = app.registry.clone();
    let project = run(move || registry.register(&request.root))
        .await
        .map_err(control_project_error)?;
    Ok(Json(WorkspaceRegistration {
        workspace_id: project.id.clone(),
    }))
}

async fn control_resolve_workspace(
    State(app): State<SharedApp>,
    headers: HeaderMap,
    Json(request): Json<RegisterWorkspaceRequest>,
) -> Result<Json<WorkspaceRegistration>, ApiError> {
    control_authorized(&headers, &app)?;
    let registry = app.registry.clone();
    let workspace_id = run(move || registry.resolve_registered(&request.root)).await?;
    Ok(Json(WorkspaceRegistration { workspace_id }))
}

fn control_protocol(protocol: u8) -> Result<(), ApiError> {
    if protocol == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ApiError(invalid(format!(
            "Unsupported connector protocol {protocol}; expected {PROTOCOL_VERSION}. Refresh the installed integration. For OpenCode, run `truespec init --integration opencode --force`."
        ))))
    }
}

fn control_project_error(error: ApiError) -> ApiError {
    if let Some(unsupported) = error.0.downcast_ref::<UnsupportedDatabaseVersion>() {
        return ApiError(invalid(unsupported.to_string()));
    }
    if let Some(duplicate) = error.0.downcast_ref::<DuplicateWorkspaceIdentity>() {
        return ApiError(conflict(duplicate.to_string()));
    }
    error
}

async fn control_connector_attach(
    State(app): State<SharedApp>,
    headers: HeaderMap,
    Json(request): Json<AttachRequest>,
) -> Result<Json<AttachResponse>, ApiError> {
    control_authorized(&headers, &app)?;
    control_protocol(request.protocol)?;
    let registry = app.registry.clone();
    let project = run(move || registry.register(&request.root))
        .await
        .map_err(control_project_error)?;
    let store = project.store.clone();
    let kind = request.kind;
    let destination = request.destination;
    let label = request.label;
    let binding = run(move || store.bind_connector(&kind, &destination, &label)).await?;
    let service_url = app
        .registration
        .lock()
        .map_err(|_| ApiError(anyhow::anyhow!("Service registration is unavailable")))?
        .url
        .clone();
    let review_path = format!("w/{}/?review={}", project.id, binding.review_id);
    let local_url = format!("{service_url}{review_path}");
    let tailnet_origin = app.config.tailnet().origin.clone();
    let url = tailnet_origin.as_ref().map_or_else(
        || local_url.clone(),
        |origin| format!("{origin}/{review_path}"),
    );
    Ok(Json(AttachResponse {
        protocol_version: PROTOCOL_VERSION,
        workspace_id: project.id.clone(),
        workspace: project.root.clone(),
        connection_id: binding.connection_id,
        review_id: binding.review_id.clone(),
        url,
        local_url,
        tailnet_origin,
    }))
}

async fn control_connector_poll(
    State(app): State<SharedApp>,
    headers: HeaderMap,
    Json(request): Json<PollRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    control_authorized(&headers, &app)?;
    control_protocol(request.protocol)?;
    if request.wait_seconds > 300 {
        return Err(ApiError(invalid(
            "Connector wait cannot exceed 300 seconds.",
        )));
    }
    let registry = app.registry.clone();
    let workspace = request.workspace_id;
    let connection = request.connection;
    let wait = Duration::from_secs(request.wait_seconds);
    let shutdown = app.shutdown_requested.clone();
    let envelope = run(move || {
        LocalInbox(registry.store(&workspace)?)
            .poll_connector_until(&connection, wait, || shutdown.load(Ordering::Acquire))
    })
    .await?;
    Ok(Json(
        serde_json::to_value(envelope).map_err(|error| ApiError(error.into()))?,
    ))
}

async fn control_connector_delivered(
    State(app): State<SharedApp>,
    headers: HeaderMap,
    Json(request): Json<DeliveryRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    control_authorized(&headers, &app)?;
    control_protocol(request.protocol)?;
    let registry = app.registry.clone();
    run(move || {
        LocalInbox(registry.store(&request.workspace_id)?).delivered(
            &request.connection,
            &request.batch_id,
            &request.lease,
            &request.receipt,
        )
    })
    .await?;
    Ok(Json(serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "event": "delivered"
    })))
}

async fn control_connector_defer(
    State(app): State<SharedApp>,
    headers: HeaderMap,
    Json(request): Json<DeferRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    control_authorized(&headers, &app)?;
    control_protocol(request.protocol)?;
    if request.delay_seconds == 0 || request.delay_seconds > 300 {
        return Err(ApiError(invalid(
            "Connector retry delay must be between 1 and 300 seconds.",
        )));
    }
    let registry = app.registry.clone();
    run(move || {
        LocalInbox(registry.store(&request.workspace_id)?).defer(
            &request.connection,
            &request.batch_id,
            &request.lease,
            request.delay_seconds,
        )
    })
    .await?;
    Ok(Json(serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "event": "deferred"
    })))
}

async fn control_connector_respond(
    State(app): State<SharedApp>,
    headers: HeaderMap,
    Json(request): Json<RespondRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    control_authorized(&headers, &app)?;
    control_protocol(request.protocol)?;
    let registry = app.registry.clone();
    run(move || {
        registry.store(&request.workspace_id)?.record_reply(
            &request.connection,
            &request.batch_id,
            &request.reply_id,
            &request.body,
        )
    })
    .await?;
    Ok(Json(serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "event": "responded"
    })))
}

async fn control_shutdown(
    State(app): State<SharedApp>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    control_authorized(&headers, &app)?;
    app.shutdown_requested.store(true, Ordering::Release);
    app.shutdown.notify_one();
    Ok(StatusCode::ACCEPTED)
}

async fn shared_project(app: &SharedApp, workspace: String) -> Result<App, ApiError> {
    let registry = app.registry.clone();
    let project = run(move || registry.project(&workspace))
        .await
        .map_err(|error| {
            use crate::service_registry::ProjectProblem;
            match error.0.downcast_ref::<ProjectProblem>() {
                Some(ProjectProblem::Missing) => {
                    ApiError(Problem::Missing("This workspace is not registered.").into())
                }
                Some(ProjectProblem::Unavailable) => ApiError(
                    Problem::Unavailable("This workspace is currently unavailable.").into(),
                ),
                None => error,
            }
        })?;
    Ok(project_app(&project))
}

fn project_app(project: &ProjectContext) -> App {
    App {
        store: project.store.clone(),
        source: project.source.clone(),
        workspace: project.root.clone(),
        source_name: project.source_name,
    }
}

async fn shared_bootstrap(
    State(shared): State<SharedApp>,
    Path(workspace): Path<String>,
) -> Result<Json<Bootstrap>, ApiError> {
    bootstrap(State(shared_project(&shared, workspace).await?)).await
}

async fn shared_documents(
    State(shared): State<SharedApp>,
    Path(workspace): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    documents(State(shared_project(&shared, workspace).await?), headers).await
}

async fn shared_document(
    State(shared): State<SharedApp>,
    Path((workspace, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    document(
        State(shared_project(&shared, workspace).await?),
        Path(id),
        headers,
    )
    .await
}

async fn shared_board(
    State(shared): State<SharedApp>,
    Path((workspace, review)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    board(
        State(shared_project(&shared, workspace).await?),
        Path(review),
        headers,
    )
    .await
}

async fn shared_add_comment(
    State(shared): State<SharedApp>,
    Path((workspace, review)): Path<(String, String)>,
    request: Json<NewComment>,
) -> Result<impl IntoResponse, ApiError> {
    add_comment(
        State(shared_project(&shared, workspace).await?),
        Path(review),
        request,
    )
    .await
}

async fn shared_add_general_comment(
    State(shared): State<SharedApp>,
    Path((workspace, review)): Path<(String, String)>,
    request: Json<GeneralComment>,
) -> Result<impl IntoResponse, ApiError> {
    add_general_comment(
        State(shared_project(&shared, workspace).await?),
        Path(review),
        request,
    )
    .await
}

async fn shared_change_comment(
    State(shared): State<SharedApp>,
    Path((workspace, review, id)): Path<(String, String, String)>,
    request: Json<CommentChange>,
) -> Result<StatusCode, ApiError> {
    change_comment(
        State(shared_project(&shared, workspace).await?),
        Path((review, id)),
        request,
    )
    .await
}

async fn shared_submit(
    State(shared): State<SharedApp>,
    Path((workspace, review)): Path<(String, String)>,
    request: Json<Submission>,
) -> Result<impl IntoResponse, ApiError> {
    submit(
        State(shared_project(&shared, workspace).await?),
        Path(review),
        request,
    )
    .await
}

async fn shared_end_session(
    State(shared): State<SharedApp>,
    Path((workspace, review)): Path<(String, String)>,
    request: Json<EndSession>,
) -> Result<StatusCode, ApiError> {
    end_session(
        State(shared_project(&shared, workspace).await?),
        Path(review),
        request,
    )
    .await
}

async fn shared_new_session(
    State(shared): State<SharedApp>,
    Path((workspace, review)): Path<(String, String)>,
) -> Result<Json<Session>, ApiError> {
    new_session(
        State(shared_project(&shared, workspace).await?),
        Path(review),
    )
    .await
}

#[derive(Serialize)]
struct Bootstrap {
    workspace: String,
    source: &'static str,
    review: Session,
}

async fn bootstrap(State(app): State<App>) -> Result<Json<Bootstrap>, ApiError> {
    run(move || {
        Ok(Bootstrap {
            workspace: app
                .workspace
                .file_name()
                .unwrap_or(app.workspace.as_os_str())
                .to_string_lossy()
                .into_owned(),
            source: app.source_name,
            review: app.store.current_session()?,
        })
    })
    .await
    .map(Json)
}

#[derive(Serialize)]
struct Entry {
    id: String,
    #[serde(flatten)]
    info: Document,
}

fn catalog(app: &App) -> Result<Vec<Entry>> {
    let documents = app.source.list()?;
    let mut ids = app
        .store
        .register(documents.iter().map(|doc| doc.path.as_str()))?;
    let mut entries = documents
        .into_iter()
        .filter_map(|info| ids.remove(&info.path).map(|id| Entry { id, info }))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| match entry.info.kind {
        "spec" => 0,
        _ if entry
            .info
            .navigation
            .first()
            .is_some_and(|segment| segment.id.starts_with("openspec:change:")) =>
        {
            1
        }
        _ if entry
            .info
            .navigation
            .first()
            .is_some_and(|segment| segment.id.starts_with("openspec:archive:")) =>
        {
            3
        }
        _ => 2,
    });
    Ok(entries)
}

async fn documents(State(app): State<App>, headers: HeaderMap) -> Result<Response, ApiError> {
    let entries = run(move || catalog(&app)).await?;
    conditional(entries, &headers)
}

#[derive(Serialize)]
struct Page {
    revision: String,
    html: String,
    headings: Vec<Heading>,
    lines: Vec<SourceLine>,
}

async fn document(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let page = run(move || {
        let path = app.store.document_path(&id)?;
        let snapshot = read_page(&app, &path)?;
        let entries = catalog(&app)?;
        let links = entries
            .iter()
            .map(|entry| (entry.info.path.as_str(), entry.id.as_str()))
            .collect::<HashMap<_, _>>();
        let content = render(&snapshot.text, |link| {
            safe_link(&path, link, |path| {
                links.get(path).map(|id| (*id).to_owned())
            })
        });
        Ok(Page {
            revision: snapshot.revision,
            html: content.html,
            headings: content.headings,
            lines: content.lines,
        })
    })
    .await?;
    conditional(page, &headers)
}

fn read_page(app: &App, path: &str) -> Result<Snapshot> {
    app.source
        .read(path)
        .map_err(|err| invalid(format!("Cannot read {path}: {err}")))?
        .ok_or_else(|| {
            Problem::Missing("This document is not available in the current source.").into()
        })
}

type EvidenceCache = HashMap<String, Option<(String, Rendered)>>;

fn locate(app: &App, comment: &Comment, cache: &mut EvidenceCache) -> (Location, Option<String>) {
    if comment.anchor.is_general() {
        return (
            Location {
                status: Status::Attached,
                target: Some(Target::General),
                source_range: None,
            },
            None,
        );
    }
    let cached = cache.entry(comment.path.clone()).or_insert_with(|| {
        app.source
            .read(&comment.path)
            .ok()
            .flatten()
            .map(|snapshot| (snapshot.revision, render(&snapshot.text, |_| None)))
    });
    match cached {
        Some((revision, rendered)) => (
            comment.anchor.locate(revision, rendered),
            Some(revision.clone()),
        ),
        None => (Location::missing(), None),
    }
}

#[derive(Serialize)]
struct BoardComment {
    #[serde(flatten)]
    comment: Comment,
    location: Location,
    source_revision: Option<String>,
}

#[derive(Serialize)]
struct Board {
    review: Session,
    latest: String,
    comments: Vec<BoardComment>,
    replies: Vec<BoardReply>,
}

#[derive(Serialize)]
struct BoardReply {
    #[serde(flatten)]
    reply: AgentReply,
    html: String,
}

async fn board(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let board = run(move || {
        let review = app.store.session(&id)?;
        let latest = app.store.current_session()?.id;
        let mut cache = EvidenceCache::new();
        let comments = app
            .store
            .comments()?
            .into_iter()
            .map(|comment| {
                let (location, source_revision) = locate(&app, &comment, &mut cache);
                BoardComment {
                    comment,
                    location,
                    source_revision,
                }
            })
            .collect();
        let replies = app
            .store
            .replies()?
            .into_iter()
            .map(|reply| {
                let html = render(&reply.body, |link| safe_link("", link, |_| None)).html;
                BoardReply { reply, html }
            })
            .collect();
        Ok(Board {
            review,
            latest,
            comments,
            replies,
        })
    })
    .await?;
    conditional(board, &headers)
}

#[derive(Deserialize)]
struct NewComment {
    document_id: String,
    revision: String,
    body: String,
    target: Target,
}

async fn add_comment(
    State(app): State<App>,
    Path(review): Path<String>,
    Json(request): Json<NewComment>,
) -> Result<impl IntoResponse, ApiError> {
    let id = run(move || {
        if matches!(
            request.target,
            Target::General | Target::Section { heading: None }
        ) {
            return Err(invalid(
                "Choose a line, paragraph, list, section, or text selection to comment.",
            ));
        }
        let path = app.store.document_path(&request.document_id)?;
        let snapshot = read_page(&app, &path)?;
        if request.revision != snapshot.revision {
            return Err(conflict(
                "The source changed while you were writing. Re-select the target and try again.",
            ));
        }
        let rendered = render(&snapshot.text, |_| None);
        let anchor = Anchor::capture(path, snapshot.revision, &rendered, request.target)
            .ok_or_else(|| {
                invalid(
                    "Cannot map this target to the source. Choose another target or select text.",
                )
            })?;
        app.store
            .create_comment(&review, &request.document_id, &request.body, &anchor)
    })
    .await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Deserialize)]
struct GeneralComment {
    body: String,
}

async fn add_general_comment(
    State(app): State<App>,
    Path(review): Path<String>,
    Json(request): Json<GeneralComment>,
) -> Result<impl IntoResponse, ApiError> {
    let id = run(move || app.store.create_general_comment(&review, &request.body)).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum CommentChange {
    Edit { version: i64, body: String },
    Remove { version: i64 },
    Resolve { resolved: bool },
}

async fn change_comment(
    State(app): State<App>,
    Path((review, id)): Path<(String, String)>,
    Json(request): Json<CommentChange>,
) -> Result<StatusCode, ApiError> {
    run(move || match request {
        CommentChange::Edit { version, body } => {
            app.store
                .edit_draft(&review, &DraftRef { id, version }, &body)
        }
        CommentChange::Remove { version } => {
            app.store.remove_draft(&review, &DraftRef { id, version })
        }
        CommentChange::Resolve { resolved } => app.store.resolve(&review, &id, resolved),
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn submit(
    State(app): State<App>,
    Path(review): Path<String>,
    Json(request): Json<Submission>,
) -> Result<impl IntoResponse, ApiError> {
    let submitted = run(move || {
        if let Some(previous) = app.store.submitted(&review, &request)? {
            return Ok(previous);
        }
        if request.drafts.is_empty() && request.general_body.is_none() {
            return Err(invalid("There are no drafts to send."));
        }
        let mut cache = EvidenceCache::new();
        let mut stale = Vec::new();
        let mut items = Vec::with_capacity(request.drafts.len());
        for draft in &request.drafts {
            let comment = app.store.comment(&draft.id)?;
            if draft.version != comment.version {
                return Err(conflict(
                    "A draft was edited after you opened this review. Refresh and try again.",
                ));
            }
            let (location, revision) = locate(&app, &comment, &mut cache);
            let general = comment.anchor.is_general();
            let status: &'static str = location.status.into();
            if matches!(location.status, Status::NeedsReview | Status::Missing) {
                stale.push((
                    comment.id.clone(),
                    comment.path.clone(),
                    revision.clone(),
                    status,
                ));
            }
            items.push(BatchItem {
                comment_id: comment.id,
                document_id: comment.document_id,
                body: comment.body,
                original: comment.anchor,
                source: SubmittedSource {
                    path: (!general).then_some(comment.path),
                    revision,
                    line: location.target.as_ref().and_then(Target::line_number),
                    anchor_status: status.into(),
                    source_range: location.source_range,
                },
            });
        }
        if !stale.is_empty() {
            let confirmation = format!("{:x}", Sha256::digest(serde_json::to_vec(&stale)?));
            if request.confirmation.as_deref() != Some(confirmation.as_str()) {
                let mut paths = stale
                    .into_iter()
                    .map(|(_, path, _, _)| path)
                    .collect::<Vec<_>>();
                paths.sort();
                paths.dedup();
                return Err(Problem::Confirm {
                    message: format!(
                        "Some drafts need review in: {}. Send their original context anyway?",
                        paths.join(", ")
                    ),
                    confirmation,
                }
                .into());
            }
        }
        app.store.submit(&review, &request, &items)
    })
    .await?;
    Ok(Json(submitted))
}

#[derive(Deserialize)]
struct EndSession {
    keep_drafts: bool,
}

async fn end_session(
    State(app): State<App>,
    Path(review): Path<String>,
    Json(request): Json<EndSession>,
) -> Result<StatusCode, ApiError> {
    run(move || app.store.end_session(&review, request.keep_drafts)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn new_session(
    State(app): State<App>,
    Path(review): Path<String>,
) -> Result<Json<Session>, ApiError> {
    run(move || {
        if app.store.session(&review)?.connector.is_some() {
            return Err(conflict(
                "Start the next paired review from your coding chat.",
            ));
        }
        app.store.next_session(&review)
    })
    .await
    .map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access() -> BrowserAccess {
        BrowserAccess {
            local_host: "127.0.0.1:47371".into(),
            local_origin: "http://127.0.0.1:47371".into(),
            tailnet_host: Some("device.example.ts.net:8443".into()),
            tailnet_origin: Some("https://device.example.ts.net:8443".into()),
        }
    }

    fn headers(host: &str, origin: Option<&str>, identity: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        if let Some(identity) = identity {
            headers.insert(
                "tailscale-user-login",
                HeaderValue::from_str(identity).unwrap(),
            );
            headers.insert("x-forwarded-for", HeaderValue::from_static("100.64.0.1"));
            headers.insert("x-forwarded-host", HeaderValue::from_str(host).unwrap());
        }
        headers
    }

    #[test]
    fn browser_trust_accepts_exact_local_and_identified_tailnet_origins() {
        let access = access();
        assert!(browser_request_allowed(
            &access,
            &Method::GET,
            "/w/workspace/",
            &headers("127.0.0.1:47371", None, None)
        ));
        assert!(browser_request_allowed(
            &access,
            &Method::POST,
            "/api/workspaces/workspace/reviews/review/comments",
            &headers(
                "device.example.ts.net:8443",
                Some("https://device.example.ts.net:8443"),
                Some("reviewer@example.com")
            )
        ));
    }

    #[test]
    fn browser_trust_rejects_missing_identity_wrong_origin_and_remote_controls() {
        let access = access();
        assert!(!browser_request_allowed(
            &access,
            &Method::GET,
            "/w/workspace/",
            &headers("device.example.ts.net:8443", None, None)
        ));
        assert!(!browser_request_allowed(
            &access,
            &Method::GET,
            "/w/workspace/",
            &headers("127.0.0.1:47371", None, Some("reviewer@example.com"))
        ));
        assert!(!browser_request_allowed(
            &access,
            &Method::POST,
            "/api/workspaces/workspace/reviews/review/comments",
            &headers(
                "device.example.ts.net:8443",
                Some("https://other.example.ts.net:8443"),
                Some("reviewer@example.com")
            )
        ));
        let mut remote_control = headers(
            "device.example.ts.net:8443",
            Some("https://device.example.ts.net:8443"),
            Some("reviewer@example.com"),
        );
        remote_control.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer private"),
        );
        assert!(!browser_request_allowed(
            &access,
            &Method::GET,
            "/__control/health",
            &remote_control
        ));
        assert!(!browser_request_allowed(
            &access,
            &Method::POST,
            "/__control/connectors/attach",
            &remote_control
        ));
    }

    #[test]
    fn local_only_browser_trust_rejects_proxy_markers_without_an_authority() {
        let access = BrowserAccess {
            local_host: "127.0.0.1:47371".into(),
            local_origin: "http://127.0.0.1:47371".into(),
            tailnet_host: None,
            tailnet_origin: None,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "tailscale-user-login",
            HeaderValue::from_static("reviewer@example.com"),
        );
        headers.insert("x-forwarded-for", HeaderValue::from_static("100.64.0.1"));
        assert!(!browser_request_allowed(
            &access,
            &Method::GET,
            "/",
            &headers
        ));
    }

    #[test]
    fn duplicate_workspace_identity_is_an_actionable_conflict() {
        let response =
            control_project_error(ApiError(DuplicateWorkspaceIdentity.into())).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
}
