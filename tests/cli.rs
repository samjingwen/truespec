use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use serde_json::{Value, json};
use uuid::Uuid;

const CLI: &str = env!("CARGO_BIN_EXE_truespec");

struct Viewer {
    root: std::path::PathBuf,
    address: String,
    workspace: String,
    review: String,
}

impl Viewer {
    fn start(root: &Path, adapter: &str, source_path: &str) -> Self {
        let root = root.canonicalize().unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        write(
            &root,
            "truespec/config/project.json",
            &format!(
                "{{\n  \"version\": 1,\n  \"source\": {{\n    \"adapter\": \"{}\",\n    \"path\": \"{}\"\n  }}\n}}\n",
                adapter, source_path
            ),
        );
        let status: Value = serde_json::from_str(&command(&root, &["up", "--json"], true)).unwrap();
        let endpoint = string(&status["endpoint"]);
        let config: Value = serde_json::from_slice(
            &fs::read(service_home(&root).join("config/truespec/service.json")).unwrap(),
        )
        .unwrap();
        let response = reqwest::blocking::Client::new()
            .post(format!("{endpoint}__control/workspaces/register"))
            .bearer_auth(string(&config["token"]))
            .json(&json!({ "root": root }))
            .send()
            .unwrap();
        assert!(
            response.status().is_success(),
            "{}",
            response.text().unwrap()
        );
        let registration: Value = response.json().unwrap();
        let mut server = Self {
            root,
            address: endpoint
                .strip_prefix("http://")
                .unwrap()
                .trim_end_matches('/')
                .to_owned(),
            workspace: string(&registration["workspace_id"]).to_owned(),
            review: String::new(),
        };
        server.review = string(&server.get("/api/bootstrap")["review"]["id"]).to_owned();
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
        let path = if let Some(suffix) = path.strip_prefix("/api/") {
            format!("/api/workspaces/{}/{}", self.workspace, suffix)
        } else {
            path.to_owned()
        };
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
        command(&self.root, &["down", "--json"], true);
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        let _ = configured_command(&self.root, &["down", "--json"]).output();
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
    let output = configured_command(root, args).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.success(), success, "{args:?}: {stderr}");
    String::from_utf8(output.stdout).unwrap()
}

fn service_home(root: &Path) -> std::path::PathBuf {
    root.canonicalize().unwrap().join(".test-service")
}

fn configured_command(root: &Path, args: &[&str]) -> Command {
    let root = root.canonicalize().unwrap();
    let home = service_home(&root);
    let mut command = Command::new(CLI);
    command
        .current_dir(root)
        .env("PATH", "")
        .env("HOME", home.join("home"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("TRUESPEC_SERVICE_PORT", "0")
        .args(args);
    command
}

fn attach(root: &Path, destination: &str) -> Value {
    serde_json::from_str(&command(
        root,
        &[
            "connector",
            "attach",
            "--kind",
            "test",
            "--destination",
            destination,
            "--label",
            "Test",
        ],
        true,
    ))
    .unwrap()
}

fn connector_poll(root: &Path, connection: &str, wait: &str) -> Value {
    serde_json::from_str(&command(
        root,
        &[
            "connector",
            "poll",
            "--connection",
            connection,
            "--wait",
            wait,
        ],
        true,
    ))
    .unwrap()
}

#[test]
fn openspec_cross_document_handoff_survives_restart() {
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
    let mut first = Viewer::start(root, "openspec", "openspec");
    let binding = attach(root, "cross-document-chat");
    let connection = string(&binding["connection_id"]).to_owned();
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
    first.stop();

    let mut second = Viewer::start(root, "markdown", "openspec");
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
    let envelope = connector_poll(root, &connection, "0");
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
        &[
            "connector",
            "delivered",
            batch,
            "--connection",
            &connection,
            "--lease",
            lease,
            "--receipt",
            "cross-document-message",
        ],
        true,
    );
    write(root, "cross-document-reply.md", "Handled both comments.\n");
    command(
        root,
        &[
            "connector",
            "respond",
            batch,
            "--connection",
            &connection,
            "--reply-id",
            "cross-document-reply",
            "--body-file",
            "cross-document-reply.md",
        ],
        true,
    );
    assert_eq!(connector_poll(root, &connection, "0")["event"], "ended");
    let mut final_viewer = Viewer::start(root, "markdown", "openspec");
    let history = final_viewer.board();
    assert_eq!(history["comments"][0]["delivery"], "replied");
    assert_eq!(history["comments"][0]["resolved"], false);
    assert_eq!(history["comments"][1]["path"], proposal);
    assert_eq!(history["comments"][1]["anchor"]["path"], proposal);
    assert_eq!(history["comments"][1]["location"]["status"], "missing");
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
    let mut viewer = Viewer::start(root, "markdown", "docs");
    let binding = attach(root, "stale-context-chat");
    let connection = string(&binding["connection_id"]).to_owned();
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
    let envelope = connector_poll(root, &connection, "0");
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
    let mut viewer = Viewer::start(root, "markdown", ".");
    let binding = attach(root, "waiting-chat");
    let mut listener = configured_command(
        root,
        &[
            "connector",
            "poll",
            "--connection",
            string(&binding["connection_id"]),
            "--wait",
            "30",
        ],
    );
    let listener = listener.stdout(Stdio::piped()).spawn().unwrap();
    viewer.post(&viewer.api("/end"), 204, json!({ "keep_drafts": false }));
    let output = listener.wait_with_output().unwrap();
    assert!(output.status.success());
    let event: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(event["event"], "ended");
    assert_eq!(
        event["next_step"],
        "The browser review has ended. Stop listening and continue in the original agent chat."
    );
    viewer.stop();
}

#[test]
fn hidden_viewer_and_attach_use_initialized_source_configuration() {
    for (mode, path, source_path) in [
        ("openspec", "openspec/specs/guide/spec.md", "openspec"),
        ("markdown", "documentation/guide.md", "documentation"),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        write(root, path, "# Guide\n");
        let mut viewer = Viewer::start(root, mode, source_path);
        assert!(
            viewer
                .get("/api/documents")
                .as_array()
                .unwrap()
                .iter()
                .any(|document| {
                    document["path"] == path
                        && document["kind"]
                            == if mode == "openspec" {
                                "spec"
                            } else {
                                "markdown"
                            }
                })
        );
        let attached: Value = serde_json::from_str(&command(
            root,
            &[
                "connector",
                "--protocol",
                "2",
                "attach",
                "--kind",
                "test",
                "--destination",
                "chat",
                "--label",
                "Test",
            ],
            true,
        ))
        .unwrap();
        assert_eq!(attached["review_id"], viewer.review);
        viewer.stop();
    }

    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(root, "guide.md", "# Guide\n");
    write(
        root,
        "truespec/config/project.json",
        "{\n  \"version\": 1,\n  \"source\": {\n    \"adapter\": \"markdown\",\n    \"path\": \".\"\n  }\n}\n",
    );
    command(
        root,
        &[
            "connector",
            "--protocol",
            "99",
            "attach",
            "--kind",
            "test",
            "--destination",
            "chat",
            "--label",
            "Test",
        ],
        false,
    );
    assert!(!root.join("truespec/.state/review.sqlite3").exists());
}

#[test]
fn public_help_exposes_only_initialization_and_service_lifecycle() {
    let help = Command::new(CLI).arg("--help").output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for public in ["init", "up", "status", "down"] {
        assert!(help.contains(&format!("  {public}")));
    }
    for hidden in [
        "service",
        "connector",
        "serve",
        "setup",
        "listen",
        "ack",
        "complete",
        "reply",
        "relink",
    ] {
        assert!(!help.contains(&format!("  {hidden}")));
        if !matches!(hidden, "service" | "connector") {
            assert!(
                !Command::new(CLI)
                    .arg(hidden)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
    }
    assert!(
        Command::new(CLI)
            .args(["connector", "--help"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn init_detects_sources_requires_the_root_and_preserves_state() {
    let fixture = tempfile::tempdir().unwrap();
    let fixture_root = fixture.path().canonicalize().unwrap();
    let root = fixture_root.as_path();
    fs::create_dir(root.join(".git")).unwrap();
    write(root, "openspec/specs/guide/spec.md", "# Guide\n");
    write(root, "truespec/.state/keep", "preserve");
    let config_home = root.join("user-config");
    let service_state = root.join("user-state");
    let service_data = root.join("user-data");
    let service_home = root.join("user-home");
    let run = |cwd: &Path, args: &[&str]| {
        Command::new(CLI)
            .current_dir(cwd)
            .env("HOME", &service_home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_STATE_HOME", &service_state)
            .env("XDG_DATA_HOME", &service_data)
            .env("TRUESPEC_SERVICE_PORT", "0")
            .args(args)
            .output()
            .unwrap()
    };

    let missing = run(root, &["init"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--integration opencode"));
    assert!(!root.join("truespec/config/project.json").exists());
    let unknown = run(root, &["init", "--integration", "missing"]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("Unknown coding integration"));
    assert!(!root.join("truespec/config/project.json").exists());

    let first = run(
        root,
        &[
            "init",
            "--integration",
            "opencode",
            "--integration",
            "opencode",
        ],
    );
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let config = fs::read_to_string(root.join("truespec/config/project.json")).unwrap();
    assert!(config.contains(r#""adapter": "openspec""#));
    assert!(config.contains(r#""path": "openspec""#));
    assert_eq!(
        fs::read_to_string(root.join("truespec/.state/keep")).unwrap(),
        "preserve"
    );
    assert!(
        run(root, &["init", "--integration", "opencode"])
            .status
            .success()
    );

    fs::create_dir(root.join("nested")).unwrap();
    let nested = run(&root.join("nested"), &["init", "--integration", "opencode"]);
    assert!(!nested.status.success());
    assert!(String::from_utf8_lossy(&nested.stderr).contains("repository root"));

    let markdown = tempfile::tempdir().unwrap();
    let markdown_root = markdown.path();
    fs::create_dir(markdown_root.join(".git")).unwrap();
    write(markdown_root, "documentation/readme.md", "# Read me\n");
    let explicit = run(
        markdown_root,
        &[
            "init",
            "--source",
            "markdown",
            "--source-path",
            "documentation",
            "--all",
        ],
    );
    assert!(
        explicit.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    let config = fs::read_to_string(markdown_root.join("truespec/config/project.json")).unwrap();
    assert!(config.contains(r#""adapter": "markdown""#));
    assert!(config.contains(r#""path": "documentation""#));
    assert!(run(root, &["down", "--json"]).status.success());
}

#[test]
fn init_installs_and_protects_the_complete_opencode_bundle() {
    let fixture = tempfile::tempdir().unwrap();
    let fixture_root = fixture.path().canonicalize().unwrap();
    let root = fixture_root.join("workspace");
    fs::create_dir_all(root.join(".git")).unwrap();
    write(&root, "guide.md", "# Guide\n");
    let config = fixture_root.join("config");
    let state = fixture_root.join("state");
    let data = fixture_root.join("data");
    let home = fixture_root.join("home");
    let install = |force| {
        let mut command = Command::new(CLI);
        command
            .current_dir(&root)
            .args(["init", "--source", "markdown", "--integration", "opencode"])
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_STATE_HOME", &state)
            .env("XDG_DATA_HOME", &data)
            .env("TRUESPEC_SERVICE_PORT", "0")
            .env("PATH", "");
        if force {
            command.arg("--force");
        }
        command.output().unwrap()
    };
    let first = install(false);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let plugin = config.join("opencode/plugins/truespec/index.ts");
    let skill = config.join("opencode/skills/truespec-review/SKILL.md");
    let generated_plugin = fs::read_to_string(&plugin).unwrap();
    let generated_skill = fs::read_to_string(&skill).unwrap();
    assert!(!generated_plugin.contains("__TRUESPEC_INSTALLED_BINARY__"));
    assert!(generated_plugin.contains("const PROTOCOL = 2"));
    assert!(!generated_plugin.contains("\"serve\""));
    assert!(!generated_plugin.contains("viewerByWorkspace"));
    assert!(generated_plugin.contains("output.url"));
    assert!(generated_plugin.contains("truespec init --integration opencode --force"));
    assert!(
        generated_plugin.contains(&Path::new(CLI).canonicalize().unwrap().display().to_string())
    );
    assert!(generated_skill.contains("name: truespec-review"));
    assert!(generated_skill.contains("Do not invoke TrueSpec transport commands yourself."));
    assert!(install(false).status.success());

    fs::remove_file(&skill).unwrap();
    assert!(install(false).status.success());
    assert_eq!(fs::read_to_string(&skill).unwrap(), generated_skill);
    assert_eq!(fs::read_to_string(&plugin).unwrap(), generated_plugin);

    fs::remove_file(&skill).unwrap();
    fs::write(&plugin, "// user-owned plugin\n").unwrap();
    let protected = install(false);
    assert!(!protected.status.success());
    assert!(!skill.exists());
    assert_eq!(
        fs::read_to_string(&plugin).unwrap(),
        "// user-owned plugin\n"
    );
    let replaced = install(true);
    assert!(replaced.status.success());
    assert_eq!(fs::read_to_string(&plugin).unwrap(), generated_plugin);
    assert_eq!(fs::read_to_string(&skill).unwrap(), generated_skill);

    fs::write(&skill, "user-owned skill\n").unwrap();
    assert!(!install(false).status.success());
    assert_eq!(fs::read_to_string(&skill).unwrap(), "user-owned skill\n");
    assert!(install(true).status.success());
    assert_eq!(fs::read_to_string(&skill).unwrap(), generated_skill);
    assert!(
        Command::new(CLI)
            .args(["down", "--json"])
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_STATE_HOME", &state)
            .env("XDG_DATA_HOME", &data)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn init_refuses_older_state_and_resets_it_only_when_explicit() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(root, "docs/guide.md", "# Guide\n");
    fs::create_dir_all(root.join("truespec/.state")).unwrap();
    let database = root.join("truespec/.state/review.sqlite3");
    let legacy = rusqlite::Connection::open(&database).unwrap();
    legacy
        .execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    legacy
        .execute(
            "INSERT INTO sessions (id, started_at) VALUES ('old-review', 1)",
            [],
        )
        .unwrap();
    drop(legacy);
    let args = [
        "init",
        "--source",
        "markdown",
        "--source-path",
        "docs",
        "--integration",
        "opencode",
    ];
    let refused = configured_command(root, &args).output().unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("init --reset-state"));
    let legacy = rusqlite::Connection::open(&database).unwrap();
    assert_eq!(
        legacy
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        legacy
            .query_row("SELECT id FROM sessions", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "old-review"
    );
    drop(legacy);

    let mut reset_args = args.to_vec();
    reset_args.push("--reset-state");
    let reset = configured_command(root, &reset_args).output().unwrap();
    assert!(
        reset.status.success(),
        "{}",
        String::from_utf8_lossy(&reset.stderr)
    );
    assert!(String::from_utf8_lossy(&reset.stderr).contains("permanently deleted"));
    let fresh = rusqlite::Connection::open(&database).unwrap();
    assert_eq!(
        fresh
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        fresh
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        fresh
            .query_row("SELECT COUNT(*) FROM workspace_identity", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        1
    );
    drop(fresh);
    command(root, &["down", "--json"], true);
}

#[test]
fn previously_saved_section_drafts_can_still_be_submitted() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    write(root, "guide.md", "# Guide\n\nOriginal context.");
    let mut viewer = Viewer::start(root, "markdown", ".");
    let binding = attach(root, "saved-section-chat");
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
        connector_poll(root, string(&binding["connection_id"]), "0")["items"][0]["original"]["original"]
            ["kind"],
        "section"
    );
}

#[test]
fn connector_accepts_a_maximum_size_reply_through_the_control_plane() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    write(root, "guide.md", "# Guide\n");
    let mut viewer = Viewer::start(root, "markdown", ".");
    let binding = attach(root, "large-reply-chat");
    let [document] = viewer
        .get("/api/documents")
        .as_array()
        .unwrap()
        .clone()
        .try_into()
        .unwrap();
    let comment = viewer.add(string(&document["id"]), 1, "Reply with the full result.");
    let submitted = viewer.post(
        &viewer.api("/submit"),
        200,
        json!({
            "key": Uuid::new_v4(),
            "end": true,
            "drafts": [{ "id": comment, "version": 1 }],
        }),
    );
    let envelope = connector_poll(root, string(&binding["connection_id"]), "0");
    command(
        root,
        &[
            "connector",
            "delivered",
            string(&submitted["batch_id"]),
            "--connection",
            string(&binding["connection_id"]),
            "--lease",
            string(&envelope["lease"]),
            "--receipt",
            "large-reply-receipt",
        ],
        true,
    );
    fs::write(root.join("large-reply.md"), "x".repeat(1024 * 1024)).unwrap();
    command(
        root,
        &[
            "connector",
            "respond",
            string(&submitted["batch_id"]),
            "--connection",
            string(&binding["connection_id"]),
            "--reply-id",
            "large-reply",
            "--body-file",
            "large-reply.md",
        ],
        true,
    );
    assert_eq!(
        viewer.board()["replies"][0]["body"].as_str().unwrap().len(),
        1024 * 1024
    );
    viewer.stop();
}

#[test]
fn connector_delivery_survives_a_temporarily_unavailable_source_tree() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    write(root, "docs/guide.md", "# Guide\n");
    let mut viewer = Viewer::start(root, "markdown", "docs");
    let binding = attach(root, "source-outage-chat");
    let docs = viewer.get("/api/documents");
    let comment = viewer.add(
        doc_id(&docs, "docs/guide.md"),
        1,
        "Handle this during an outage.",
    );
    let submitted = viewer.post(
        &viewer.api("/submit"),
        200,
        json!({
            "key": Uuid::new_v4(),
            "end": true,
            "drafts": [{ "id": comment, "version": 1 }],
        }),
    );
    viewer.stop();
    fs::rename(root.join("docs"), root.join("docs-offline")).unwrap();

    let first = connector_poll(root, string(&binding["connection_id"]), "0");
    command(
        root,
        &[
            "connector",
            "defer",
            string(&submitted["batch_id"]),
            "--connection",
            string(&binding["connection_id"]),
            "--lease",
            string(&first["lease"]),
            "--retry-after",
            "1",
        ],
        true,
    );
    std::thread::sleep(Duration::from_millis(1_100));
    let retried = connector_poll(root, string(&binding["connection_id"]), "2");
    command(
        root,
        &[
            "connector",
            "delivered",
            string(&submitted["batch_id"]),
            "--connection",
            string(&binding["connection_id"]),
            "--lease",
            string(&retried["lease"]),
            "--receipt",
            "source-outage-receipt",
        ],
        true,
    );
    write(
        root,
        "source-outage-reply.md",
        "Handled while the source was unavailable.\n",
    );
    command(
        root,
        &[
            "connector",
            "respond",
            string(&submitted["batch_id"]),
            "--connection",
            string(&binding["connection_id"]),
            "--reply-id",
            "source-outage-reply",
            "--body-file",
            "source-outage-reply.md",
        ],
        true,
    );

    fs::rename(root.join("docs-offline"), root.join("docs")).unwrap();
    let mut restored = Viewer::start(root, "markdown", "docs");
    assert_eq!(
        restored.board()["replies"][0]["body"],
        "Handled while the source was unavailable.\n"
    );
    restored.stop();
}

#[test]
fn a_versioned_connector_owns_its_batches_across_review_rounds() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let path = "openspec/specs/reader/spec.md";
    write(root, path, "# Reader\n\nOriginal text.\n");
    fs::create_dir(root.join(".git")).unwrap();
    let mut viewer = Viewer::start(root, "openspec", "openspec");
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
                "2",
                "attach",
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
    let binding: Value = serde_json::from_str(&attach(root, "test-chat", true)).unwrap();
    assert_eq!(binding["protocol_version"], 2);
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
    assert_eq!(viewer.board()["review"]["connector"], "Test Harness");
    assert!(!viewer.board().to_string().contains("test-chat"));
    let adopted = connector_poll("0");
    assert_eq!(adopted["batch_id"], manual["batch_id"]);
    let adopted_id = string(&adopted["batch_id"]);
    record(&adopted, string(&adopted["lease"]), "adopted-message", true);
    write(root, "adopted-reply.md", "Handled adopted feedback.\n");
    command(
        root,
        &[
            "connector",
            "respond",
            adopted_id,
            "--connection",
            connection,
            "--reply-id",
            "adopted-agent-message",
            "--body-file",
            "adopted-reply.md",
        ],
        true,
    );
    assert_eq!(viewer.board()["comments"][0]["delivery"], "replied");
    assert_eq!(connector_poll("0")["event"], "idle");
    let first = add_and_submit("Clarify the first revision.", false);
    let first_attempt = connector_poll("0");
    assert_eq!(first_attempt["batch_id"], first["batch_id"]);
    assert_eq!(first_attempt["session_ended"], false);
    let first_id = string(&first_attempt["batch_id"]);
    let expired_lease = string(&first_attempt["lease"]);
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
        viewer.board()["replies"][1]["body"],
        "Updated the first revision.\n"
    );
    assert_eq!(
        viewer.board()["replies"][1]["comment_ids"][0],
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
    assert_eq!(viewer.board()["replies"].as_array().unwrap().len(), 3);
    assert_eq!(viewer.board()["comments"][1]["delivery"], "replied");
    assert_eq!(viewer.board()["comments"][1]["resolved"], false);
    viewer.post(&viewer.api("/new"), 409, json!({}));
    let next: Value = serde_json::from_str(&attach(root, "another-chat", true)).unwrap();
    assert_ne!(next["review_id"], binding["review_id"]);
    assert_eq!(connector_poll("0")["event"], "ended");
    viewer.stop();
}
