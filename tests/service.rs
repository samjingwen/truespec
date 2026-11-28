use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use uuid::Uuid;

const CLI: &str = env!("CARGO_BIN_EXE_truespec");

struct ServiceFixture {
    root: tempfile::TempDir,
    base: PathBuf,
    port: u16,
}

impl ServiceFixture {
    fn new(port: u16) -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        Self { root, base, port }
    }

    fn command(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(CLI);
        command
            .current_dir(cwd)
            .env("PATH", "")
            .env("HOME", self.base.join("home"))
            .env("XDG_CONFIG_HOME", self.base.join("config"))
            .env("XDG_STATE_HOME", self.base.join("state"))
            .env("XDG_DATA_HOME", self.base.join("data"))
            .env("TRUESPEC_SERVICE_PORT", self.port.to_string())
            .args(args);
        command
    }

    fn run(&self, args: &[&str], success: bool) -> Output {
        let output = self.command(&self.base, args).output().unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{args:?}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn json(&self, args: &[&str]) -> Value {
        serde_json::from_slice(&self.run(args, true).stdout).unwrap()
    }

    fn config(&self) -> PathBuf {
        self.base.join("config/truespec/service.json")
    }

    fn registration(&self) -> PathBuf {
        self.base.join("state/truespec/service.json")
    }

    fn log(&self) -> PathBuf {
        self.base.join("state/truespec/service.log")
    }

    fn repository(&self, name: &str) -> PathBuf {
        let root = self.base.join(name);
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
        root
    }
}

impl Drop for ServiceFixture {
    fn drop(&mut self) {
        let _ = &self.root;
        let _ = self.command(&self.base, &["down", "--json"]).output();
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn available_port() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.local_addr().unwrap().port()
}

#[test]
fn detached_service_lifecycle_is_idempotent_contested_and_restartable() {
    let fixture = ServiceFixture::new(available_port());
    assert_eq!(fixture.json(&["status", "--json"])["state"], "stopped");

    let first = fixture.json(&["up", "--json"]);
    assert_eq!(first["state"], "running");
    let first_pid = first["pid"].as_u64().unwrap();
    assert_eq!(fixture.json(&["up", "--json"])["pid"], first_pid);
    let human = text(&fixture.run(&["status"], true));
    let config: Value = serde_json::from_slice(&fs::read(fixture.config()).unwrap()).unwrap();
    assert!(human.contains("TrueSpec service: running"));
    assert!(!human.contains(config["token"].as_str().unwrap()));
    assert!(!human.contains(fixture.base.to_str().unwrap()));
    thread::sleep(Duration::from_millis(200));
    assert_eq!(fixture.json(&["status", "--json"])["pid"], first_pid);

    assert_eq!(fixture.json(&["down", "--json"])["state"], "stopped");
    assert_eq!(fixture.json(&["down", "--json"])["state"], "stopped");

    let contenders = (0..6)
        .map(|_| {
            let mut command = fixture.command(&fixture.base, &["up", "--json"]);
            command
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let statuses = contenders
        .into_iter()
        .map(|child| child.wait_with_output().unwrap())
        .collect::<Vec<_>>();
    for status in &statuses {
        assert!(status.status.success(), "{}", text(status));
    }
    let pids = statuses
        .iter()
        .map(|status| {
            serde_json::from_slice::<Value>(&status.stdout).unwrap()["pid"]
                .as_u64()
                .unwrap()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(pids.len(), 1);
    let contested_pid = *pids.first().unwrap();
    assert_ne!(contested_pid, first_pid);

    assert_eq!(fixture.json(&["down", "--json"])["state"], "stopped");
    let restarted = fixture.json(&["up", "--json"]);
    assert_eq!(restarted["state"], "running");
    assert_ne!(restarted["pid"].as_u64().unwrap(), contested_pid);

    let log = fs::read_to_string(fixture.log()).unwrap();
    assert!(!log.contains(config["token"].as_str().unwrap()));
    assert!(!log.contains(fixture.base.to_str().unwrap()));
}

#[test]
fn stale_registration_is_repaired_without_adopting_an_endpoint_owner() {
    let fixture = ServiceFixture::new(0);
    fixture.json(&["status", "--json"]);
    fs::create_dir_all(fixture.registration().parent().unwrap()).unwrap();
    fs::write(
        fixture.registration(),
        serde_json::to_vec_pretty(&json!({
            "protocol": 1,
            "version": env!("CARGO_PKG_VERSION"),
            "instance_id": Uuid::new_v4(),
            "generation": Uuid::new_v4(),
            "pid": u32::MAX,
            "url": "http://127.0.0.1:9/",
            "state": "ready"
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(fixture.json(&["status", "--json"])["state"], "stale");
    assert_eq!(fixture.json(&["up", "--json"])["state"], "running");

    fixture.json(&["down", "--json"]);
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let collision = ServiceFixture::new(listener.local_addr().unwrap().port());
    assert_eq!(
        collision.json(&["status", "--json"])["state"],
        "endpoint_collision"
    );
    let failed = collision.run(&["up", "--json"], false);
    assert!(text(&failed).contains("occupied by an unverified process"));
    assert!(listener.local_addr().is_ok());
}

#[test]
fn incompatible_authenticated_service_is_reported_and_left_running() {
    let fixture = ServiceFixture::new(0);
    fixture.json(&["status", "--json"]);
    let config: Value = serde_json::from_slice(&fs::read(fixture.config()).unwrap()).unwrap();
    let token = config["token"].as_str().unwrap().to_owned();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let instance = Uuid::new_v4().to_string();
    let generation = Uuid::new_v4().to_string();
    fs::create_dir_all(fixture.registration().parent().unwrap()).unwrap();
    fs::write(
        fixture.registration(),
        serde_json::to_vec_pretty(&json!({
            "protocol": 99,
            "version": "99.0.0",
            "instance_id": instance,
            "generation": generation,
            "pid": std::process::id(),
            "url": url,
            "state": "ready"
        }))
        .unwrap(),
    )
    .unwrap();

    let server = thread::spawn(move || {
        for stream in listener.incoming().take(2) {
            let mut stream = stream.unwrap();
            let mut request = [0_u8; 4096];
            let size = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..size]);
            assert!(request.contains(&format!("authorization: Bearer {token}")));
            let body = serde_json::to_string(&json!({
                "protocol": 99,
                "version": "99.0.0",
                "instance_id": instance,
                "pid": std::process::id(),
                "state": "ready",
                "projects": 0,
                "open_reviews": 0
            }))
            .unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });

    assert_eq!(fixture.json(&["status", "--json"])["state"], "incompatible");
    let failed = fixture.run(&["up", "--json"], false);
    assert!(text(&failed).contains("different TrueSpec version"));
    server.join().unwrap();
}

#[test]
fn init_starts_the_service_only_after_preflight_and_registers_the_workspace() {
    let fixture = ServiceFixture::new(0);
    let repository = fixture.repository("project");
    let initialized = fixture
        .command(
            &repository,
            &[
                "init",
                "--source",
                "markdown",
                "--source-path",
                "docs",
                "--integration",
                "opencode",
            ],
        )
        .output()
        .unwrap();
    assert!(initialized.status.success(), "{}", text(&initialized));
    assert!(repository.join("truespec/config/project.json").is_file());
    assert!(repository.join("truespec/.state/review.sqlite3").is_file());
    let status = fixture.json(&["status", "--json"]);
    assert_eq!(status["state"], "running");
    assert_eq!(status["projects"], 1);

    fixture.json(&["down", "--json"]);
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let blocked = ServiceFixture::new(listener.local_addr().unwrap().port());
    let repository = blocked.repository("blocked-project");
    let failed = blocked
        .command(
            &repository,
            &[
                "init",
                "--source",
                "markdown",
                "--source-path",
                "docs",
                "--integration",
                "opencode",
            ],
        )
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(!repository.join("truespec/config/project.json").exists());
    assert!(!repository.join("truespec/.state/review.sqlite3").exists());
    assert!(
        !blocked
            .base
            .join("config/opencode/plugins/truespec/index.ts")
            .exists()
    );
}

#[test]
fn down_cancels_an_active_connector_poll_and_exits_gracefully() {
    let fixture = ServiceFixture::new(available_port());
    let repository = fixture.repository("polling-project");
    let initialized = fixture
        .command(
            &repository,
            &[
                "init",
                "--source",
                "markdown",
                "--source-path",
                "docs",
                "--integration",
                "opencode",
            ],
        )
        .output()
        .unwrap();
    assert!(initialized.status.success(), "{}", text(&initialized));
    let attached = fixture
        .command(
            &repository,
            &[
                "connector",
                "attach",
                "--kind",
                "test",
                "--destination",
                "polling-chat",
                "--label",
                "Test",
            ],
        )
        .output()
        .unwrap();
    assert!(attached.status.success(), "{}", text(&attached));
    let binding: Value = serde_json::from_slice(&attached.stdout).unwrap();
    let mut poll = fixture.command(
        &repository,
        &[
            "connector",
            "poll",
            "--connection",
            binding["connection_id"].as_str().unwrap(),
            "--wait",
            "300",
        ],
    );
    let poll = poll
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(500));

    let started = Instant::now();
    assert_eq!(fixture.json(&["down", "--json"])["state"], "stopped");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "graceful shutdown waited too long for a cancellable poll"
    );
    let output = poll.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["event"], "idle");
}

#[test]
fn reset_holds_maintenance_ownership_against_connector_recovery() {
    let fixture = ServiceFixture::new(available_port());
    let repository = fixture.repository("reset-project");
    let init_args = [
        "init",
        "--source",
        "markdown",
        "--source-path",
        "docs",
        "--integration",
        "opencode",
    ];
    let initialized = fixture.command(&repository, &init_args).output().unwrap();
    assert!(initialized.status.success(), "{}", text(&initialized));
    let attached = fixture
        .command(
            &repository,
            &[
                "connector",
                "attach",
                "--kind",
                "test",
                "--destination",
                "reset-chat",
                "--label",
                "Test",
            ],
        )
        .output()
        .unwrap();
    assert!(attached.status.success(), "{}", text(&attached));
    let first: Value = serde_json::from_slice(&attached.stdout).unwrap();
    let destination = "reset-chat".to_owned();

    let running = Arc::new(AtomicBool::new(true));
    let recovery = {
        let running = running.clone();
        let fixture_base = fixture.base.clone();
        let repository = repository.clone();
        let port = fixture.port;
        let destination = destination.clone();
        thread::spawn(move || {
            while running.load(Ordering::Acquire) {
                let _ = Command::new(CLI)
                    .current_dir(&repository)
                    .env("PATH", "")
                    .env("HOME", fixture_base.join("home"))
                    .env("XDG_CONFIG_HOME", fixture_base.join("config"))
                    .env("XDG_STATE_HOME", fixture_base.join("state"))
                    .env("XDG_DATA_HOME", fixture_base.join("data"))
                    .env("TRUESPEC_SERVICE_PORT", port.to_string())
                    .args([
                        "connector",
                        "attach",
                        "--kind",
                        "test",
                        "--destination",
                        &destination,
                        "--label",
                        "Test",
                    ])
                    .output();
                thread::sleep(Duration::from_millis(25));
            }
        })
    };
    thread::sleep(Duration::from_millis(200));
    let mut reset_args = init_args.to_vec();
    reset_args.push("--reset-state");
    let reset = fixture.command(&repository, &reset_args).output().unwrap();
    running.store(false, Ordering::Release);
    recovery.join().unwrap();
    assert!(reset.status.success(), "{}", text(&reset));

    let attached = fixture
        .command(
            &repository,
            &[
                "connector",
                "attach",
                "--kind",
                "test",
                "--destination",
                &destination,
                "--label",
                "Test",
            ],
        )
        .output()
        .unwrap();
    assert!(attached.status.success(), "{}", text(&attached));
    let fresh: Value = serde_json::from_slice(&attached.stdout).unwrap();
    assert_ne!(fresh["workspace_id"], first["workspace_id"]);
}
