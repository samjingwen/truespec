use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    sync::Arc,
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

use crate::{
    anchor::{Anchor, Location, Status, Target},
    markdown::{Heading, Rendered, SourceLine, render, safe_link},
    review::{
        AgentReply, BatchItem, Comment, DraftRef, Problem, Session, Store, Submission,
        SubmittedSource, conflict, invalid,
    },
    source::{Document, DocumentSource, Snapshot},
};

#[derive(Clone)]
struct App {
    store: Store,
    source: Arc<dyn DocumentSource>,
    workspace: PathBuf,
    source_name: &'static str,
}

pub async fn serve(
    store: Store,
    source: Box<dyn DocumentSource>,
    workspace: PathBuf,
    source_name: &'static str,
    port: u16,
    json: bool,
) -> Result<()> {
    let review = store.current_session()?;
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).await?;
    let address = listener.local_addr()?.to_string();
    let origin = format!("http://{address}");
    let app = Router::new()
        .route(
            "/",
            get(|| async {
                asset(
                    "text/html; charset=utf-8",
                    include_str!("../web/index.html"),
                )
            }),
        )
        .route(
            "/app.js",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async { asset("text/css; charset=utf-8", include_str!("../web/app.css")) }),
        )
        .route(
            "/targets.js",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_str!("../web/targets.js"),
                )
            }),
        )
        .route(
            "/favicon.svg",
            get(|| async { asset("image/svg+xml", include_str!("../web/favicon.svg")) }),
        )
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/documents", get(documents))
        .route("/api/documents/{id}", get(document))
        .route("/api/reviews/{review}", get(board))
        .route("/api/reviews/{review}/comments", post(add_comment))
        .route("/api/reviews/{review}/general", post(add_general_comment))
        .route("/api/reviews/{review}/comments/{id}", post(change_comment))
        .route("/api/reviews/{review}/submit", post(submit))
        .route("/api/reviews/{review}/end", post(end_session))
        .route("/api/reviews/{review}/new", post(new_session))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn_with_state(
            (address, origin.clone()),
            local_only,
        ))
        .with_state(App {
            store,
            source: source.into(),
            workspace: workspace.clone(),
            source_name,
        });
    if json {
        let startup = serde_json::json!({
            "protocol_version": crate::inbox::PROTOCOL_VERSION,
            "url": format!("{origin}/"),
            "workspace": workspace,
            "review_id": review.id,
        });
        println!("{}", serde_json::to_string(&startup)?);
    } else {
        println!("TrueSpec is listening at {origin}/");
        println!("Review: {}", review.id);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
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

async fn local_only(
    State((host, origin)): State<(String, String)>,
    request: Request,
    next: Next,
) -> Response {
    let host_matches = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        == Some(host.as_str());
    let safe = matches!(*request.method(), Method::GET | Method::HEAD);
    let origin_matches = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        == Some(origin.as_str());
    if !host_matches || (!safe && !origin_matches) {
        return (
            StatusCode::FORBIDDEN,
            "Use the TrueSpec loopback URL from the terminal.",
        )
            .into_response();
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
            Some(Problem::Confirm {
                message,
                confirmation,
            }) => (StatusCode::CONFLICT, message.as_str(), Some(confirmation)),
            None => {
                eprintln!("TrueSpec request failed: {:#}", self.0);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "TrueSpec could not finish the request. Check the serve terminal and try again.",
                    None,
                )
            }
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
