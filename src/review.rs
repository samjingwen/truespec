use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior::Immediate, params,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::anchor::{Anchor, Status};

const STORE_PATH: &str = "truespec/.state/review.sqlite3";
const STATE_PATH: &str = "truespec/.state";
const GENERAL_DOCUMENT_ID: &str = "truespec-general-feedback";
const GENERAL_DOCUMENT_PATH: &str = "@truespec/general-feedback";
const SCHEMA: &str = include_str!("schema.sql");
const CONNECTOR_MIGRATION: &str = include_str!("schema-v2.sql");
const FEED_MIGRATION: &str = include_str!("schema-v3.sql");
const MANUAL_REPLY_MIGRATION: &str = include_str!("schema-v4.sql");

#[derive(Debug)]
pub enum Problem {
    Invalid(String),
    Missing(&'static str),
    Conflict(String),
    Confirm {
        message: String,
        confirmation: String,
    },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) | Self::Conflict(s) | Self::Confirm { message: s, .. } => {
                f.write_str(s)
            }
            Self::Missing(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for Problem {}

pub fn invalid(message: impl Into<String>) -> anyhow::Error {
    Problem::Invalid(message.into()).into()
}

pub fn conflict(message: impl Into<String>) -> anyhow::Error {
    Problem::Conflict(message.into()).into()
}

#[derive(Clone)]
pub struct Store(PathBuf);

pub fn agent_root(root: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = root {
        let root = root
            .canonicalize()
            .with_context(|| format!("Cannot find workspace {}", root.display()))?;
        ensure!(root.is_dir(), "Workspace must be a directory");
        return Ok(root);
    }
    let cwd = std::env::current_dir()?.canonicalize()?;
    cwd.ancestors()
        .find(|path| path.join(STORE_PATH).is_file())
        .map(Path::to_path_buf)
        .context("No TrueSpec review database found. Run from the workspace or pass --root.")
}

pub fn reset_state(root: &Path) -> Result<()> {
    let root = root
        .canonicalize()
        .with_context(|| format!("Cannot find workspace {}", root.display()))?;
    ensure!(root.is_dir(), "Workspace must be a directory");
    let truespec = root.join("truespec");
    let state = root.join(STATE_PATH);
    let metadata = match fs::symlink_metadata(&state) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("TrueSpec state is already empty: {}", state.display());
            return Ok(());
        }
        Err(error) => return Err(error).context("Cannot inspect TrueSpec state"),
    };
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Refusing to reset a TrueSpec state path that is not a real directory: {}",
        state.display()
    );
    ensure!(
        truespec.canonicalize()? == truespec && state.canonicalize()? == state,
        "Refusing to reset TrueSpec state through a symlink"
    );
    eprintln!("Removing TrueSpec state: {}", state.display());
    fs::remove_dir_all(&state).context("Cannot remove TrueSpec state")?;
    Ok(())
}

pub fn now() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub ended: bool,
    pub connector: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Binding {
    pub connection_id: String,
    pub review_id: String,
}

#[derive(Debug, Serialize)]
pub struct Comment {
    pub sequence: i64,
    pub id: String,
    pub document_id: String,
    pub path: String,
    pub body: String,
    pub anchor: Anchor,
    pub version: i64,
    pub resolved: bool,
    pub batch_id: Option<String>,
    pub delivery: Option<&'static str>,
    pub connector: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentReply {
    pub sequence: i64,
    pub id: String,
    pub batch_id: String,
    pub body: String,
    pub created_at: i64,
    pub comment_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DraftRef {
    pub id: String,
    pub version: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Submission {
    pub key: Uuid,
    pub drafts: Vec<DraftRef>,
    pub end: bool,
    pub confirmation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchItem {
    pub comment_id: String,
    pub document_id: String,
    pub body: String,
    pub original: Anchor,
    pub source: SubmittedSource,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmittedSource {
    pub path: Option<String>,
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub anchor_status: String,
    pub source_range: Option<std::ops::Range<usize>>,
}

impl From<Status> for &'static str {
    fn from(status: Status) -> Self {
        match status {
            Status::Attached => "attached",
            Status::Relocated => "relocated",
            Status::NeedsReview => "needs_review",
            Status::Missing => "missing",
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Submitted {
    pub batch_id: String,
    pub count: usize,
    pub ended: bool,
}

impl Store {
    pub fn open(root: &Path, create: bool) -> Result<Self> {
        let path = root.join(STORE_PATH);
        if create {
            for dir in ["truespec", "truespec/.state"] {
                let dir = root.join(dir);
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                if dir.ends_with(".state") {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                match builder.create(&dir) {
                    Ok(()) => {}
                    Err(error)
                        if error.kind() == std::io::ErrorKind::AlreadyExists && dir.is_dir() => {}
                    Err(error) => {
                        return Err(error).context("Cannot create TrueSpec state directory");
                    }
                }
                ensure!(
                    dir.canonicalize()? == dir,
                    "TrueSpec state directories must not be symlinks"
                );
            }
        } else {
            ensure!(path.is_file(), "No review database in {}", root.display());
        }
        ensure!(
            path.parent().unwrap().canonicalize()? == path.parent().unwrap(),
            "TrueSpec state directory must not be a symlink"
        );
        if fs::symlink_metadata(&path).is_ok() {
            ensure!(
                path.canonicalize()? == path && path.is_file(),
                "Invalid TrueSpec database path"
            );
        }
        let store = Self(path);
        let mut con = store.connection(create)?;
        let version = con.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
        if version == 0 && create {
            con.pragma_update(None, "journal_mode", "WAL")?;
        }
        if version != 4 {
            let tx = con.transaction_with_behavior(Immediate)?;
            match tx.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))? {
                0 if create => {
                    tx.execute_batch(SCHEMA)?;
                    tx.execute_batch(CONNECTOR_MIGRATION)?;
                    tx.execute_batch(FEED_MIGRATION)?;
                    tx.execute_batch(MANUAL_REPLY_MIGRATION)?;
                }
                1 => {
                    tx.execute_batch(CONNECTOR_MIGRATION)?;
                    tx.execute_batch(FEED_MIGRATION)?;
                    tx.execute_batch(MANUAL_REPLY_MIGRATION)?;
                }
                2 => {
                    tx.execute_batch(FEED_MIGRATION)?;
                    tx.execute_batch(MANUAL_REPLY_MIGRATION)?;
                }
                3 => tx.execute_batch(MANUAL_REPLY_MIGRATION)?,
                4 => {}
                version => anyhow::bail!("Unsupported TrueSpec database version {version}"),
            }
            tx.commit()?;
        }
        Ok(store)
    }

    fn connection(&self, create: bool) -> Result<Connection> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let flags = if create {
            flags | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            flags
        };
        let con = Connection::open_with_flags(&self.0, flags)?;
        con.busy_timeout(Duration::from_secs(5))?;
        con.pragma_update(None, "foreign_keys", "ON")?;
        con.pragma_update(None, "synchronous", "FULL")?;
        Ok(con)
    }

    pub(crate) fn connect(&self) -> Result<Connection> {
        self.connection(false)
    }

    pub(crate) fn write<T>(
        &self,
        execute: impl FnOnce(&Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut con = self.connect()?;
        let tx = con.transaction_with_behavior(Immediate)?;
        let result = execute(&tx)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn current_session(&self) -> Result<Session> {
        if let Some(session) = current(&self.connect()?)? {
            return Ok(session);
        }
        self.write(|tx| match current(tx)? {
            Some(session) => Ok(session),
            None => create_review(tx),
        })
    }

    pub fn session(&self, id: &str) -> Result<Session> {
        read_session(&self.connect()?, id)
    }

    pub fn bind_connector(&self, kind: &str, destination: &str, label: &str) -> Result<Binding> {
        validate_connector_text("Connector kind", kind, 64)?;
        validate_connector_text("Destination", destination, 1024)?;
        validate_connector_text("Connector label", label, 96)?;
        self.write(|tx| {
            let review = match current(tx)? {
                Some(review) if !review.ended => review,
                _ => create_review(tx)?,
            };
            let bound = tx.query_row(
                "SELECT id, kind, destination FROM review_connectors WHERE session_id = ?1",
                [&review.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            ).optional()?;
            let connection_id = if let Some((id, old_kind, old_destination)) = bound {
                if old_kind != kind || old_destination != destination {
                    return Err(conflict("The open review is paired with a different chat. End it before pairing a new chat."));
                }
                id
            } else {
                let id = new_id();
                tx.execute(
                    "INSERT INTO review_connectors (id, session_id, kind, destination, label) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, review.id, kind, destination, label],
                )?;
                id
            };
            Ok(Binding { connection_id, review_id: review.id })
        })
    }

    pub fn connector_review(&self, connection_id: &str) -> Result<String> {
        self.connect()?
            .query_row(
                "SELECT session_id FROM review_connectors WHERE id = ?1",
                [connection_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Problem::Missing("Review connector not found.").into())
    }

    pub fn next_session(&self, previous: &str) -> Result<Session> {
        self.write(|tx| {
            let latest = current(tx)?.ok_or(Problem::Missing("Review not found."))?;
            if latest.id != previous {
                return Err(conflict(
                    "A newer review already exists. Open it from the viewer.",
                ));
            }
            if !latest.ended {
                return Err(conflict("End this review before starting a new one."));
            }
            create_review(tx)
        })
    }

    pub fn end_session(&self, id: &str, keep_drafts: bool) -> Result<()> {
        self.write(|tx| {
            if read_session(tx, id)?.ended {
                return Ok(());
            }
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM comments WHERE batch_id IS NULL", [], |row| row.get(0))?;
            if count > 0 && !keep_drafts {
                return Err(conflict("There are unsent drafts. Confirm that you want to keep them for a later review."));
            }
            end_review(tx, id)
        })
    }

    pub fn register<'a>(
        &self,
        paths: impl IntoIterator<Item = &'a str>,
    ) -> Result<HashMap<String, String>> {
        let requested: Vec<_> = paths.into_iter().collect();
        let con = self.connect()?;
        let registered = registered_documents(&con)?;
        if requested.iter().all(|path| registered.contains_key(*path)) {
            return Ok(registered);
        }
        drop(con);
        self.write(|tx| {
            let mut insert = tx.prepare(
                "INSERT INTO documents (id, path) VALUES (?1, ?2) ON CONFLICT(path) DO NOTHING",
            )?;
            for path in requested
                .iter()
                .filter(|path| !registered.contains_key(**path))
            {
                insert.execute(params![new_id(), path])?;
            }
            registered_documents(tx)
        })
    }

    pub fn document_path(&self, id: &str) -> Result<String> {
        self.connect()?
            .query_row("SELECT path FROM documents WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| Problem::Missing("Document not found.").into())
    }

    pub fn relink(&self, from: &str, to: &str) -> Result<String> {
        self.write(|tx| {
            let Some(id): Option<String> = tx
                .query_row("SELECT id FROM documents WHERE path = ?1", [from], |row| {
                    row.get(0)
                })
                .optional()?
            else {
                return Err(
                    Problem::Missing("The old path has no TrueSpec document identity.").into(),
                );
            };
            if let Some(destination) = tx
                .query_row("SELECT id FROM documents WHERE path = ?1", [to], |row| {
                    row.get::<_, String>(0)
                })
                .optional()?
            {
                let used: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM comments WHERE document_id = ?1)",
                    [&destination],
                    |row| row.get(0),
                )?;
                if used {
                    return Err(conflict(
                        "The destination already has its own review history.",
                    ));
                }
                tx.execute("DELETE FROM documents WHERE id = ?1", [&destination])?;
            }
            tx.execute(
                "UPDATE documents SET path = ?2 WHERE id = ?1",
                params![id, to],
            )?;
            Ok(id)
        })
    }

    pub fn comments(&self) -> Result<Vec<Comment>> {
        let con = self.connect()?;
        let mut stmt = con.prepare(&format!("{COMMENTS} ORDER BY f.sequence"))?;
        stmt.query_map([], read_comment)?
            .collect::<rusqlite::Result<_>>()
            .map_err(Into::into)
    }

    pub fn replies(&self) -> Result<Vec<AgentReply>> {
        let con = self.connect()?;
        let mut stmt = con.prepare(
            "SELECT f.sequence, r.id, r.batch_id, r.body, r.created_at
             FROM agent_replies r JOIN review_feed f
             ON f.kind = 'agent_reply' AND f.reference_id = r.id
             ORDER BY f.sequence",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(sequence, id, batch_id, body, created_at)| {
                let mut links = con.prepare(
                    "SELECT comment_id FROM batch_items WHERE batch_id = ?1 ORDER BY ordinal",
                )?;
                let comment_ids = links
                    .query_map([&batch_id], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(AgentReply {
                    sequence,
                    id,
                    batch_id,
                    body,
                    created_at,
                    comment_ids,
                })
            })
            .collect()
    }

    pub fn record_reply(
        &self,
        connection: &str,
        batch_id: &str,
        reply_id: &str,
        body: &str,
    ) -> Result<()> {
        self.record_reply_inner(Some(connection), batch_id, reply_id, body)
    }

    pub fn record_manual_reply(&self, batch_id: &str, reply_id: &str, body: &str) -> Result<()> {
        self.record_reply_inner(None, batch_id, reply_id, body)
    }

    fn record_reply_inner(
        &self,
        connection: Option<&str>,
        batch_id: &str,
        reply_id: &str,
        body: &str,
    ) -> Result<()> {
        validate_connector_text("Reply ID", reply_id, 1024)?;
        validate_body_limit("Agent reply", body, 1024 * 1024)?;
        self.write(|tx| {
            let existing = tx
                .query_row(
                    "SELECT id, body FROM agent_replies WHERE batch_id = ?1",
                    [batch_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if let Some((id, existing_body)) = existing {
                return if id == reply_id && existing_body == body {
                    Ok(())
                } else {
                    Err(conflict("This batch already has a different agent reply."))
                };
            }
            let belongs: bool = if let Some(connection) = connection {
                tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM batches
                     WHERE id = ?1 AND connector_id = ?2 AND connector_receipt IS NOT NULL)",
                    params![batch_id, connection],
                    |row| row.get(0),
                )?
            } else {
                tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM batches
                     WHERE id = ?1 AND connector_id IS NULL AND first_received_at IS NOT NULL)",
                    [batch_id],
                    |row| row.get(0),
                )?
            };
            if !belongs {
                return Err(conflict(match connection {
                    Some(_) => "The delivered batch does not belong to this connector.",
                    None => "Acknowledge this manual batch before storing its reply.",
                }));
            }
            let created = now();
            tx.execute(
                "INSERT INTO agent_replies (id, batch_id, connector_id, body, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![reply_id, batch_id, connection, body, created],
            )?;
            insert_feed(tx, "agent_reply", reply_id)?;
            tx.execute(
                "UPDATE batch_items SET completed_at = COALESCE(completed_at, ?2),
                 completed_token = COALESCE(completed_token, ?3) WHERE batch_id = ?1",
                params![batch_id, created, format!("reply:{reply_id}")],
            )?;
            Ok(())
        })
    }

    pub fn comment(&self, id: &str) -> Result<Comment> {
        self.connect()?
            .query_row(&format!("{COMMENTS} WHERE c.id = ?1"), [id], read_comment)
            .optional()?
            .ok_or_else(|| Problem::Missing("Comment not found.").into())
    }

    pub fn create_comment(
        &self,
        session: &str,
        document: &str,
        body: &str,
        anchor: &Anchor,
    ) -> Result<String> {
        validate_body(body)?;
        let id = new_id();
        self.write(|tx| {
            require_open(tx, session)?;
            let path: String = tx.query_row(
                "SELECT path FROM documents WHERE id = ?1",
                [document],
                |row| row.get(0),
            )?;
            if path != anchor.path {
                return Err(conflict(
                    "The document was relinked. Refresh before saving the comment.",
                ));
            }
            tx.execute(
                "INSERT INTO comments (id, document_id, body, anchor) VALUES (?1, ?2, ?3, ?4)",
                params![id, document, body, serde_json::to_string(anchor)?],
            )?;
            insert_feed(tx, "comment", &id)?;
            Ok(id)
        })
    }

    pub fn create_general_comment(&self, session: &str, body: &str) -> Result<String> {
        validate_body(body)?;
        let id = new_id();
        let anchor = Anchor::general();
        self.write(|tx| {
            require_open(tx, session)?;
            tx.execute(
                "INSERT INTO documents (id, path) VALUES (?1, ?2) ON CONFLICT(id) DO NOTHING",
                params![GENERAL_DOCUMENT_ID, GENERAL_DOCUMENT_PATH],
            )?;
            tx.execute(
                "INSERT INTO comments (id, document_id, body, anchor) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id,
                    GENERAL_DOCUMENT_ID,
                    body,
                    serde_json::to_string(&anchor)?
                ],
            )?;
            insert_feed(tx, "comment", &id)?;
            Ok(id)
        })
    }

    pub fn edit_draft(&self, session: &str, draft: &DraftRef, body: &str) -> Result<()> {
        validate_body(body)?;
        self.write(|tx| {
            require_open(tx, session)?;
            let count = tx.execute("UPDATE comments SET body = ?3, version = version + 1 WHERE id = ?1 AND version = ?2 AND batch_id IS NULL",
                params![draft.id, draft.version, body])?;
            check_draft(count)
        })
    }

    pub fn remove_draft(&self, session: &str, draft: &DraftRef) -> Result<()> {
        self.write(|tx| {
            require_open(tx, session)?;
            let count = tx.execute(
                "DELETE FROM comments WHERE id = ?1 AND version = ?2 AND batch_id IS NULL",
                params![draft.id, draft.version],
            )?;
            check_draft(count)
        })
    }

    pub fn resolve(&self, session: &str, id: &str, resolved: bool) -> Result<()> {
        self.write(|tx| {
            require_open(tx, session)?;
            if tx.execute(
                "UPDATE comments SET resolved = ?2 WHERE id = ?1 AND batch_id IS NOT NULL",
                params![id, resolved],
            )? != 1
            {
                return Err(Problem::Missing("Submitted annotation not found.").into());
            }
            Ok(())
        })
    }

    pub fn submitted(&self, session: &str, request: &Submission) -> Result<Option<Submitted>> {
        replay(&self.connect()?, session, request)
    }

    pub fn submit(
        &self,
        session: &str,
        request: &Submission,
        items: &[BatchItem],
    ) -> Result<Submitted> {
        self.write(|tx| {
            if let Some(submitted) = replay(tx, session, request)? {
                return Ok(submitted);
            }
            require_open(tx, session)?;
            if items.is_empty() || items.len() != request.drafts.len() {
                return Err(invalid("Choose at least one draft to send."));
            }
            let batch_id = new_id();
            tx.execute("INSERT INTO batches (id, session_id, request_key, request, ended, created, item_count, connector_id)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, (SELECT id FROM review_connectors WHERE session_id = ?2))", params![
                batch_id, session, request.key.to_string(), serde_json::to_string(request)?, request.end, now(), i64::try_from(items.len())?
            ])?;
            for (ordinal, (draft, item)) in request.drafts.iter().zip(items).enumerate() {
                ensure!(draft.id == item.comment_id, "Draft and snapshot do not match");
                let count = if item.original.is_general() {
                    ensure!(
                        item.source.path.is_none() && item.document_id == GENERAL_DOCUMENT_ID,
                        "General feedback source does not match"
                    );
                    tx.execute(
                        "UPDATE comments SET batch_id = ?3 WHERE id = ?1 AND version = ?2
                         AND batch_id IS NULL AND document_id = ?4",
                        params![draft.id, draft.version, batch_id, item.document_id],
                    )?
                } else {
                    let path = item
                        .source
                        .path
                        .as_deref()
                        .context("Annotated feedback requires a source path")?;
                    tx.execute("UPDATE comments SET batch_id = ?3 WHERE id = ?1 AND version = ?2
                        AND batch_id IS NULL AND document_id = ?4 AND (SELECT path FROM documents WHERE id = ?4) = ?5",
                        params![draft.id, draft.version, batch_id, item.document_id, path])?
                };
                check_draft(count)?;
                tx.execute("INSERT INTO batch_items (batch_id, comment_id, ordinal, snapshot) VALUES (?1, ?2, ?3, ?4)",
                    params![batch_id, item.comment_id, i64::try_from(ordinal)?, serde_json::to_string(item)?])?;
            }
            if request.end {
                end_review(tx, session)?;
            }
            Ok(Submitted { batch_id, count: items.len(), ended: request.end })
        })
    }
}

fn registered_documents(con: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = con.prepare("SELECT path, id FROM documents")?;
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()
        .map_err(Into::into)
}

fn read_session(con: &Connection, id: &str) -> Result<Session> {
    con.query_row(
        "SELECT s.id, s.ended_at IS NOT NULL, c.label FROM sessions s
            LEFT JOIN review_connectors c ON c.session_id = s.id WHERE s.id = ?1",
        [id],
        session_row,
    )
    .optional()?
    .ok_or_else(|| Problem::Missing("Review not found.").into())
}

fn current(con: &Connection) -> Result<Option<Session>> {
    Ok(con
        .query_row(
            "SELECT s.id, s.ended_at IS NOT NULL, c.label FROM sessions s
                LEFT JOIN review_connectors c ON c.session_id = s.id ORDER BY s.rowid DESC LIMIT 1",
            [],
            session_row,
        )
        .optional()?)
}

fn session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        ended: row.get(1)?,
        connector: row.get(2)?,
    })
}

fn create_review(con: &Connection) -> Result<Session> {
    let id = new_id();
    con.execute(
        "INSERT INTO sessions(id, started_at) VALUES (?1, ?2)",
        params![id, now()],
    )?;
    Ok(Session {
        id,
        ended: false,
        connector: None,
    })
}

fn require_open(con: &Connection, id: &str) -> Result<()> {
    if read_session(con, id)?.ended {
        return Err(conflict(
            "This review has ended. Open a new review to make changes.",
        ));
    }
    Ok(())
}

fn end_review(con: &Connection, id: &str) -> Result<()> {
    con.execute(
        "UPDATE sessions SET ended_at = ?2 WHERE id = ?1 AND ended_at IS NULL",
        params![id, now()],
    )?;
    Ok(())
}

fn validate_body(body: &str) -> Result<()> {
    validate_body_limit("Comment", body, 32_768)
}

fn validate_body_limit(field: &str, body: &str, limit: usize) -> Result<()> {
    if body.trim().is_empty() || body.len() > limit {
        return Err(invalid(format!("{field} must contain 1 to {limit} bytes.")));
    }
    Ok(())
}

pub fn validate_connector_text(field: &str, value: &str, limit: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        return Err(invalid(format!(
            "{field} must contain 1 to {limit} bytes of non-control text."
        )));
    }
    Ok(())
}

fn check_draft(changes: usize) -> Result<()> {
    if changes != 1 {
        return Err(conflict(
            "A draft changed, moved, or was already sent. Refresh the review and try again.",
        ));
    }
    Ok(())
}

fn insert_feed(con: &Connection, kind: &str, reference: &str) -> Result<()> {
    con.execute(
        "INSERT INTO review_feed (kind, reference_id) VALUES (?1, ?2)",
        params![kind, reference],
    )?;
    Ok(())
}

fn replay(con: &Connection, session: &str, request: &Submission) -> Result<Option<Submitted>> {
    let result = con.query_row("SELECT id, request, item_count, ended FROM batches WHERE session_id = ?1 AND request_key = ?2",
        params![session, request.key.to_string()],
        |row| Ok((row.get(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get(3)?))
    ).optional()?;
    if let Some((batch_id, original, count, ended)) = result {
        if original != serde_json::to_string(request)? {
            return Err(conflict("This submission key is already in use."));
        }
        return Ok(Some(Submitted {
            batch_id,
            count: usize::try_from(count)?,
            ended,
        }));
    }
    Ok(None)
}

const COMMENTS: &str = "SELECT f.sequence, c.id, c.document_id, d.path, c.body, c.anchor, c.version, c.resolved, c.batch_id,
    CASE WHEN c.batch_id IS NULL THEN NULL WHEN ar.id IS NOT NULL THEN 'replied'
     WHEN i.completed_at IS NOT NULL THEN 'completed'
     WHEN b.connector_receipt IS NOT NULL THEN 'delivered'
     WHEN b.retry_at IS NOT NULL THEN 'connector_retry'
    WHEN b.lease_received_at IS NOT NULL AND b.lease_until > CAST(unixepoch('subsec') * 1000 AS INTEGER) THEN 'received'
     WHEN b.first_received_at IS NOT NULL THEN 'retry' ELSE 'queued' END, rc.label
     FROM comments c JOIN review_feed f ON f.kind = 'comment' AND f.reference_id = c.id
     JOIN documents d ON d.id = c.document_id LEFT JOIN batches b ON b.id = c.batch_id
     LEFT JOIN batch_items i ON i.comment_id = c.id LEFT JOIN review_connectors rc ON rc.id = b.connector_id
     LEFT JOIN agent_replies ar ON ar.batch_id = b.id";

fn read_comment(row: &rusqlite::Row<'_>) -> rusqlite::Result<Comment> {
    let anchor = row.get::<_, String>(5)?;
    let anchor = serde_json::from_str(&anchor).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let delivery = row
        .get::<_, Option<String>>(9)?
        .map(|status| match status.as_str() {
            "received" => "received",
            "delivered" => "delivered",
            "replied" => "replied",
            "completed" => "completed",
            "retry" => "retry",
            "connector_retry" => "connector_retry",
            _ => "queued",
        });
    Ok(Comment {
        sequence: row.get(0)?,
        id: row.get(1)?,
        document_id: row.get(2)?,
        path: row.get(3)?,
        body: row.get(4)?,
        anchor,
        version: row.get(6)?,
        resolved: row.get(7)?,
        batch_id: row.get(8)?,
        delivery,
        connector: row.get(10)?,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn reset_refuses_a_symlinked_state_directory() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let outside = root.join("outside");
        fs::create_dir_all(root.join("truespec")).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), "do not delete").unwrap();
        symlink(&outside, root.join(STATE_PATH)).unwrap();

        assert!(reset_state(&root).is_err());
        assert_eq!(
            fs::read_to_string(outside.join("keep")).unwrap(),
            "do not delete"
        );
    }
}
