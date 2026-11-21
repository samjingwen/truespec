use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use serde_json::{Value, json};
use uuid::Uuid;

const CLI: &str = env!("CARGO_BIN_EXE_truespec");

struct Viewer {
    child: Child,
    address: String,
    review: String,
}

impl Viewer {
    fn start(cwd: &Path, args: &[&str]) -> Self {
        let mut child = Command::new(CLI)
            .current_dir(cwd)
            .env("PATH", "")
            .arg("serve")
            .args(args)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        let address = line
            .trim()
            .strip_prefix("TrueSpec is listening at http://")
            .unwrap()
            .trim_end_matches('/')
            .to_owned();
        assert!(address.starts_with("127.0.0.1:"));
        line.clear();
        out.read_line(&mut line).unwrap();
        let review = line.trim().strip_prefix("Review: ").unwrap().to_owned();
        let server = Self {
            child,
            address,
            review,
        };
        assert_eq!(server.http("GET", "/", &[], "").0, 200);
        server
    }

    fn http(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (u16, String, String) {
        let mut client = TcpStream::connect(&self.address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(client, "{method} {path} HTTP/1.0\r\nConnection: close\r\n").unwrap();
        if !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("host"))
        {
            write!(client, "Host: {}\r\n", self.address).unwrap();
        }
        for (name, value) in headers {
            write!(client, "{name}: {value}\r\n").unwrap();
        }
        write!(client, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let code = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (code, headers.to_owned(), body.to_owned())
    }

    fn get(&self, path: &str) -> Value {
        let (code, _, body) = self.http("GET", path, &[], "");
        assert_eq!(code, 200, "{path}: {body}");
        serde_json::from_str(&body).unwrap()
    }

    fn post(&self, path: &str, expected: u16, data: Value) -> Value {
        let origin = format!("http://{}", self.address);
        let (code, _, body) = self.http(
            "POST",
            path,
            &[("Origin", &origin), ("Content-Type", "application/json")],
            &data.to_string(),
        );
        assert_eq!(code, expected, "{path}: {body}");
        if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&body).unwrap()
        }
    }

    fn board(&self) -> Value {
        self.get(&self.api(""))
    }

    fn api(&self, suffix: &str) -> String {
        format!("/api/reviews/{}{suffix}", self.review)
    }

    fn add(&self, id: &str, line: u64, body: &str) -> String {
        let page = self.get(&format!("/api/documents/{id}"));
        let item = self.post(
            &self.api("/comments"),
            201,
            json!({
                "document_id": id,
                "revision": page["revision"],
                "body": body,
                "target": { "kind": "line", "line": line },
            }),
        );
        string(&item["id"]).to_owned()
    }

    fn stop(&mut self) {
        #[cfg(unix)]
        {
            assert!(
                Command::new("/bin/kill")
                    .args(["-TERM", &self.child.id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(self.child.wait().unwrap().success());
        }
        #[cfg(not(unix))]
        {
            self.child.kill().unwrap();
            self.child.wait().unwrap();
        }
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn string(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn doc_id<'a>(docs: &'a Value, path: &str) -> &'a str {
    string(
        &docs
            .as_array()
            .unwrap()
            .iter()
            .find(|doc| doc["path"] == path)
            .unwrap()["id"],
    )
}

fn command(root: &Path, args: &[&str], success: bool) -> String {
    let output = Command::new(CLI)
        .current_dir(root)
        .env("PATH", "")
        .args(args)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.success(), success, "{args:?}: {stderr}");
    String::from_utf8(output.stdout).unwrap()
}

fn poll(root: &Path, review: &str) -> Value {
    serde_json::from_str(&command(
        root,
        &["poll", "--session", review, "--json", "--wait", "0"],
        true,
    ))
    .unwrap()
}

#[test]
fn openspec_nested_workspace_and_cross_document_handoff_survive_restart() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let spec = "openspec/specs/login/spec.md";
    let proposal = "openspec/changes/tweak/proposal.md";
    write(
        root,
        spec,
        "# Login\n\n## Timeout\n\nA clear statement.\n\n[Proposal](../../changes/tweak/proposal.md#scope)\n",
    );
    write(
        root,
        proposal,
        "# Update login\n\n## Scope\n\nA clear scope.\n",
    );
    write(
        root,
        "openspec/changes/tweak/specs/login/spec.md",
        "## ADDED Requirements\n\nDraft",
    );
    write(
        root,
        "openspec/changes/tweak/sketch.md",
        "Markdown outside the default schema",
    );
    write(root, "openspec/changes/archive/older/tasks.md", "# Tasks");
    let mut first = Viewer::start(&root.join("openspec/specs/login"), &[]);
    let docs = first.get("/api/documents");
    assert_eq!(docs.as_array().unwrap().len(), 5);
    assert!(
        docs.as_array()
            .unwrap()
            .iter()
            .any(|doc| doc["kind"] == "delta")
    );
    assert!(
        docs.as_array()
            .unwrap()
            .iter()
            .any(|doc| doc["kind"] == "archived")
    );
    let one = doc_id(&docs, spec);
    let two = doc_id(&docs, proposal);
    let page = first.get(&format!("/api/documents/{one}"));
    assert!(string(&page["html"]).contains("#scope"));
    let one_comment = first.add(one, 5, "Check the boundary.");
    let two_comment = first.add(two, 5, "Clarify the scope.");
    assert_eq!(poll(root, &first.review)["event"], "idle");
    first.stop();

    let mut second = Viewer::start(root, &["--source", "markdown", "--docs", "openspec"]);
    assert_eq!(second.review, first.review);
    let docs = second.get("/api/documents");
    assert_eq!(doc_id(&docs, spec), one);
    assert_eq!(doc_id(&docs, proposal), two);
    assert_eq!(second.board()["comments"].as_array().unwrap().len(), 2);
    let drafts = json!([{ "id": one_comment, "version": 1 }, { "id": two_comment, "version": 1 }]);
    let wrong = json!({ "key": Uuid::new_v4(), "end": true, "drafts": [
        { "id": one_comment, "version": 1 }, { "id": two_comment, "version": 100 }
    ] });
    second.post(&second.api("/submit"), 409, wrong);
    assert_eq!(second.board()["review"]["ended"], false);
    assert_eq!(poll(root, &second.review)["event"], "idle");
    let send = json!({ "key": Uuid::new_v4(), "end": true, "drafts": drafts });
    let sent = second.post(&second.api("/submit"), 200, send.clone());
    assert_eq!(sent["count"], 2);
    assert_eq!(second.post(&second.api("/submit"), 200, send), sent);
    assert_eq!(second.board()["comments"][0]["delivery"], "queued");
    assert_eq!(second.board()["review"]["ended"], true);
    second.post(
        &second.api(&format!("/comments/{one_comment}")),
        409,
        json!({ "action": "resolve", "resolved": true }),
    );
    second.stop();

    write(root, spec, "Edited after submission.\n");
    fs::rename(
        root.join(proposal),
        root.join("openspec/changes/tweak/renamed.md"),
    )
    .unwrap();
    let moved = "openspec/changes/tweak/renamed.md";
    let relink = command(root, &["relink", proposal, moved], true);
    assert!(relink.contains(two));
    let envelope = poll(root, &second.review);
    assert_eq!(envelope["event"], "batch");
    assert_eq!(envelope["session_ended"], true);
    let batch = string(&envelope["batch_id"]);
    let lease = string(&envelope["lease"]);
    assert_eq!(envelope["items"][0]["comment_id"], one_comment);
    assert_eq!(envelope["items"][0]["source"]["revision"], page["revision"]);
    assert_eq!(envelope["items"][0]["source"]["line"], 5);
    assert_eq!(envelope["items"][1]["comment_id"], two_comment);
    assert_eq!(envelope["items"][1]["source"]["path"], proposal);
    assert_eq!(envelope["items"][1]["body"], "Clarify the scope.");
    command(
        root,
        &["complete", batch, &one_comment, "--lease", lease],
        false,
    );
    command(root, &["ack", batch, "--lease", lease], true);
    for item in [&one_comment, &one_comment, &two_comment] {
        command(root, &["complete", batch, item, "--lease", lease], true);
    }
    assert_eq!(poll(root, &second.review)["event"], "ended");
    let mut final_viewer = Viewer::start(root, &["--root", root.to_str().unwrap()]);
    let history = final_viewer.board();
    assert_eq!(history["comments"][0]["delivery"], "completed");
    assert_eq!(history["comments"][0]["resolved"], false);
    assert_eq!(history["comments"][1]["path"], moved);
    assert_eq!(history["comments"][1]["anchor"]["path"], proposal);
    let new_review = final_viewer.post(&final_viewer.api("/new"), 200, json!({}));
    final_viewer.post(
        &format!(
            "/api/reviews/{}/comments/{one_comment}",
            string(&new_review["id"])
        ),
        204,
        json!({
            "action": "resolve", "resolved": true,
        }),
    );
    assert_eq!(final_viewer.board()["comments"][0]["resolved"], true);
    final_viewer.stop();
}

#[test]
fn markdown_refresh_and_stale_confirmation_keep_the_original_context() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let source = "docs/spec.md";
    write(
        root,
        source,
        "# Spec\n\nA stable statement.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n<script>throw 1</script>\n",
    );
    write(root, "elsewhere.md", "# Not part of this source");
    let mut viewer = Viewer::start(
        root,
        &[
            "--root",
            root.to_str().unwrap(),
            "--source",
            "markdown",
            "--docs",
            "docs",
        ],
    );
    let docs = viewer.get("/api/documents");
    assert_eq!(docs.as_array().unwrap().len(), 1);
    let id = doc_id(&docs, source);
    let path = format!("/api/documents/{id}");
    let (code, headers, body) = viewer.http("GET", &path, &[], "");
    assert_eq!(code, 200);
    assert!(!body.contains("<script>"));
    assert!(body.contains("<table>"));
    let etag = headers
        .lines()
        .find_map(|line| line.strip_prefix("etag: "))
        .unwrap();
    assert_eq!(
        viewer.http("GET", &path, &[("If-None-Match", etag)], "").0,
        304
    );
    assert!(
        viewer
            .http("GET", "/app.js", &[], "")
            .2
            .contains("initialize")
    );
    assert_eq!(
        viewer.http("GET", "/", &[("Host", "other.invalid")], "").0,
        403
    );
    assert_eq!(
        viewer
            .http(
                "POST",
                &viewer.api("/end"),
                &[("Origin", "http://other.invalid")],
                ""
            )
            .0,
        403
    );

    viewer.post(
        &viewer.api("/comments"),
        422,
        json!({
            "document_id": id,
            "revision": viewer.get(&path)["revision"],
            "body": "This should not queue a section.",
            "target": { "kind": "section", "heading": null },
        }),
    );
    let id = viewer.add(id, 1, "Clarify the statement.");
    viewer.post(&viewer.api("/end"), 409, json!({ "keep_drafts": false }));
    viewer.post(&viewer.api("/end"), 204, json!({ "keep_drafts": true }));
    assert_eq!(poll(root, &viewer.review)["event"], "ended");
    let next = viewer.post(&viewer.api("/new"), 200, json!({}));
    viewer.review = string(&next["id"]).to_owned();
    assert_eq!(
        viewer.board()["comments"][0]["body"],
        "Clarify the statement."
    );
    write(
        root,
        source,
        "# After rewrite\n\nNo matching section remains.",
    );
    assert_eq!(
        viewer.http("GET", &path, &[("If-None-Match", etag)], "").0,
        200
    );
    assert_eq!(
        viewer.board()["comments"][0]["location"]["status"],
        "needs_review"
    );
    let mut send =
        json!({ "key": Uuid::new_v4(), "end": false, "drafts": [{ "id": id, "version": 1 }] });
    let before = viewer.post(&viewer.api("/submit"), 409, send.clone());
    assert!(string(&before["confirmation"]).len() > 10);
    send["confirmation"] = before["confirmation"].clone();
    viewer.post(&viewer.api("/submit"), 200, send);
    viewer.stop();
    let envelope = poll(root, &viewer.review);
    assert_eq!(envelope["items"][0]["original"]["heading"][0], "Spec");
    assert_eq!(envelope["items"][0]["original"]["original"]["line"], 1);
    assert_eq!(
        envelope["items"][0]["source"]["anchor_status"],
        "needs_review"
    );
}

#[test]
fn a_waiting_agent_exits_when_a_browser_review_ends_without_feedback() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    write(root, "guide.md", "# Guide");
    let mut viewer = Viewer::start(root, &["--source", "markdown"]);
    let polling = Command::new(CLI)
        .current_dir(root)
        .env("PATH", "")
        .args([
            "poll",
            "--session",
            &viewer.review,
            "--json",
            "--wait",
            "30",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    viewer.post(&viewer.api("/end"), 204, json!({ "keep_drafts": false }));
    let output = polling.wait_with_output().unwrap();
    assert!(output.status.success());
    let event: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(event["event"], "ended");
    assert!(string(&event["next_step"]).contains("original agent chat"));
    viewer.stop();
}

#[test]
fn setup_opencode_installs_idempotently_and_protects_changes() {
    let fixture = tempfile::tempdir().unwrap();
    let config = fixture.path().join("config");
    let install = || {
        Command::new(CLI)
            .args(["setup", "--connector", "opencode"])
            .env("XDG_CONFIG_HOME", &config)
            .output()
            .unwrap()
    };
    let listed = Command::new(CLI)
        .args(["setup", "--list"])
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(listed.status.success());
    assert!(String::from_utf8_lossy(&listed.stdout).contains("opencode"));
    assert!(String::from_utf8_lossy(&listed.stdout).contains("not installed"));
    let missing = Command::new(CLI)
        .arg("setup")
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stdout).contains("Available coding harness connectors")
    );
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--connector opencode"));
    let first = install();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let plugin = config.join("opencode/plugins/truespec/index.ts");
    let generated = fs::read_to_string(&plugin).unwrap();
    assert!(!generated.contains("__TRUESPEC_INSTALLED_BINARY__"));
    assert!(generated.contains(&Path::new(CLI).canonicalize().unwrap().display().to_string()));
    assert!(install().status.success());
    let all = Command::new(CLI)
        .args(["setup", "--all"])
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(all.status.success());

    fs::write(&plugin, "// user-owned plugin\n").unwrap();
    let protected = install();
    assert!(!protected.status.success());
    assert_eq!(
        fs::read_to_string(&plugin).unwrap(),
        "// user-owned plugin\n"
    );
    let replaced = Command::new(CLI)
        .args(["setup", "--connector", "opencode", "--force"])
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(replaced.status.success());
    assert_eq!(fs::read_to_string(&plugin).unwrap(), generated);
}

#[test]
fn reset_state_starts_fresh_without_touching_documents_or_config() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let document = "docs/guide.md";
    write(root, document, "# Guide\n\nReview this.\n");
    write(root, "truespec/config/truespec.yaml", "source: markdown\n");
    let mut first = Viewer::start(root, &["--source", "markdown", "--docs", "docs"]);
    let docs = first.get("/api/documents");
    let id = doc_id(&docs, document);
    first.add(id, 3, "This draft should be reset.");
    let previous_review = first.review.clone();
    assert_eq!(first.board()["comments"].as_array().unwrap().len(), 1);
    first.stop();

    let mut reset = Viewer::start(
        root,
        &["--source", "markdown", "--docs", "docs", "--reset-state"],
    );
    assert_ne!(reset.review, previous_review);
    assert!(reset.board()["comments"].as_array().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(root.join(document)).unwrap(),
        "# Guide\n\nReview this.\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("truespec/config/truespec.yaml")).unwrap(),
        "source: markdown\n"
    );
    assert!(root.join("truespec/.state/review.sqlite3").is_file());
    reset.stop();
}

#[test]
fn previously_saved_section_drafts_can_still_be_submitted() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    write(root, "guide.md", "# Guide\n\nOriginal context.");
    let mut viewer = Viewer::start(root, &["--source", "markdown"]);
    let docs = viewer.get("/api/documents");
    let document_id = doc_id(&docs, "guide.md");
    let revision = viewer.get(&format!("/api/documents/{document_id}"))["revision"].clone();
    let anchor = json!({
        "path": "guide.md", "revision": revision, "heading": ["Guide"],
        "quote": null, "before": "", "after": "Original context.", "source_range": { "start": 0, "end": 26 },
        "original": { "kind": "section", "heading": "guide" }, "unique": true,
    });
    let comment_id = Uuid::new_v4().to_string();
    let db = rusqlite::Connection::open(root.join("truespec/.state/review.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO comments (id, document_id, body, anchor) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            comment_id,
            document_id,
            "A saved section draft.",
            anchor.to_string()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO review_feed (kind, reference_id) VALUES ('comment', ?1)",
        [&comment_id],
    )
    .unwrap();
    assert_eq!(
        viewer.board()["comments"][0]["location"]["target"]["kind"],
        "section"
    );
    viewer.post(
        &viewer.api("/submit"),
        200,
        json!({
            "key": Uuid::new_v4(), "end": true, "drafts": [{ "id": comment_id, "version": 1 }],
        }),
    );
    viewer.stop();
    assert_eq!(
        poll(root, &viewer.review)["items"][0]["original"]["original"]["kind"],
        "section"
    );
}

#[test]
fn a_versioned_connector_owns_its_batches_across_review_rounds() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let path = "openspec/specs/reader/spec.md";
    write(root, path, "# Reader\n\nOriginal text.\n");
    fs::create_dir(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("truespec/.state")).unwrap();
    {
        let legacy =
            rusqlite::Connection::open(root.join("truespec/.state/review.sqlite3")).unwrap();
        legacy
            .execute_batch(include_str!("../src/schema.sql"))
            .unwrap();
        legacy
            .execute(
                "INSERT INTO sessions (id, started_at) VALUES ('legacy-review', 1)",
                [],
            )
            .unwrap();
    }
    let mut viewer = Viewer::start(root, &[]);
    assert_eq!(viewer.review, "legacy-review");
    let docs = viewer.get("/api/documents");
    let id = doc_id(&docs, path);
    let add_and_submit = |body: &str, end: bool| {
        let item = viewer.add(id, 3, body);
        viewer.post(
            &viewer.api("/submit"),
            200,
            json!({
                "key": Uuid::new_v4(), "end": end, "drafts": [{ "id": item, "version": 1 }],
            }),
        )
    };
    let manual = add_and_submit("Originally sent to the manual inbox.", false);
    let attach = |cwd: &Path, destination: &str, success| {
        command(
            cwd,
            &[
                "connector",
                "--protocol",
                "1",
                "attach",
                "--source",
                "openspec",
                "--kind",
                "test-harness",
                "--destination",
                destination,
                "--label",
                "Test Harness",
            ],
            success,
        )
    };
    let binding: Value =
        serde_json::from_str(&attach(&root.join("openspec/specs"), "test-chat", true)).unwrap();
    assert_eq!(binding["protocol_version"], 1);
    assert_eq!(binding["review_id"], viewer.review);
    assert_eq!(
        serde_json::from_str::<Value>(&attach(root, "test-chat", true)).unwrap(),
        binding
    );
    attach(root, "another-chat", false);
    command(
        root,
        &[
            "connector",
            "--protocol",
            "22",
            "poll",
            "--connection",
            "any",
            "--wait",
            "0",
        ],
        false,
    );
    let nested = root.join("nested-checkout");
    fs::create_dir_all(nested.join(".git")).unwrap();
    attach(&nested, "test-chat", false);

    let connection = string(&binding["connection_id"]);
    let connector_poll = |seconds| {
        serde_json::from_str::<Value>(&command(
            root,
            &[
                "connector",
                "poll",
                "--connection",
                connection,
                "--wait",
                seconds,
            ],
            true,
        ))
        .unwrap()
    };
    assert_eq!(viewer.board()["review"]["connector"], "Test Harness");
    assert!(!viewer.board().to_string().contains("test-chat"));
    assert_eq!(connector_poll("0")["event"], "idle");
    let first = add_and_submit("Clarify the first revision.", false);
    assert_eq!(poll(root, &viewer.review)["batch_id"], manual["batch_id"]);
    assert_eq!(poll(root, &viewer.review)["event"], "idle");
    let first_attempt = connector_poll("0");
    assert_eq!(first_attempt["batch_id"], first["batch_id"]);
    assert_eq!(first_attempt["session_ended"], false);
    let first_id = string(&first_attempt["batch_id"]);
    let expired_lease = string(&first_attempt["lease"]);
    command(root, &["ack", first_id, "--lease", expired_lease], false);
    command(
        root,
        &[
            "connector",
            "defer",
            first_id,
            "--connection",
            connection,
            "--lease",
            expired_lease,
            "--retry-after",
            "1",
        ],
        true,
    );
    assert_eq!(viewer.board()["comments"][1]["delivery"], "connector_retry");

    write(
        root,
        path,
        "# Reader\n\nUpdated Markdown in the same review.\n",
    );
    let next_revision = viewer.get(&format!("/api/documents/{id}"))["revision"].clone();
    assert_ne!(
        next_revision,
        first_attempt["items"][0]["source"]["revision"]
    );
    let second = add_and_submit("Clarify the second revision.", true);
    assert_eq!(viewer.board()["review"]["ended"], true);
    let retry = connector_poll("5");
    assert_eq!(retry["batch_id"], first["batch_id"]);
    assert_eq!(retry["attempt"], 2);
    assert_eq!(retry["items"], first_attempt["items"]);

    let record = |incoming: &Value, lease: &str, receipt: &str, success| {
        command(
            root,
            &[
                "connector",
                "delivered",
                string(&incoming["batch_id"]),
                "--connection",
                connection,
                "--lease",
                lease,
                "--receipt",
                receipt,
            ],
            success,
        )
    };
    record(&retry, expired_lease, "first-message", false);
    let lease = string(&retry["lease"]);
    record(&retry, lease, "first-message", true);
    record(&retry, lease, "first-message", true);
    record(&retry, lease, "different-receipt", false);
    write(root, "agent-reply.md", "Updated the first revision.\n");
    command(
        root,
        &[
            "connector",
            "respond",
            first_id,
            "--connection",
            connection,
            "--reply-id",
            "agent-message-1",
            "--body-file",
            "agent-reply.md",
        ],
        true,
    );
    command(
        root,
        &[
            "connector",
            "respond",
            first_id,
            "--connection",
            connection,
            "--reply-id",
            "agent-message-1",
            "--body-file",
            "agent-reply.md",
        ],
        true,
    );
    assert_eq!(
        viewer.board()["replies"][0]["body"],
        "Updated the first revision.\n"
    );
    assert_eq!(
        viewer.board()["replies"][0]["comment_ids"][0],
        first_attempt["items"][0]["comment_id"]
    );
    assert_eq!(viewer.board()["comments"][1]["delivery"], "replied");
    write(root, "other-reply.md", "A different response.\n");
    command(
        root,
        &[
            "connector",
            "respond",
            first_id,
            "--connection",
            connection,
            "--reply-id",
            "agent-message-2",
            "--body-file",
            "other-reply.md",
        ],
        false,
    );
    let final_batch = connector_poll("0");
    assert_eq!(final_batch["batch_id"], second["batch_id"]);
    assert_eq!(final_batch["review_id"], viewer.review);
    assert_eq!(final_batch["session_ended"], true);
    assert_eq!(final_batch["items"][0]["source"]["revision"], next_revision);
    record(
        &final_batch,
        string(&final_batch["lease"]),
        "final-message",
        true,
    );
    command(
        root,
        &[
            "connector",
            "respond",
            string(&final_batch["batch_id"]),
            "--connection",
            connection,
            "--reply-id",
            "agent-message-3",
            "--body-file",
            "other-reply.md",
        ],
        true,
    );
    assert_eq!(connector_poll("0")["event"], "ended");
    assert_eq!(viewer.board()["replies"].as_array().unwrap().len(), 2);
    assert_eq!(viewer.board()["comments"][1]["delivery"], "replied");
    assert_eq!(viewer.board()["comments"][1]["resolved"], false);
    viewer.post(&viewer.api("/new"), 409, json!({}));
    let next: Value = serde_json::from_str(&attach(root, "another-chat", true)).unwrap();
    assert_ne!(next["review_id"], binding["review_id"]);
    assert_eq!(connector_poll("0")["event"], "ended");
    viewer.stop();
}
