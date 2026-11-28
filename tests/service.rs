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

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
        let tailscale = self.base.join("tailscale");
        if tailscale.is_file() {
            command.env("TRUESPEC_TAILSCALE_BIN", tailscale);
        }
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

    fn tailnet_state(&self) -> PathBuf {
        self.base.join("tailscale-state.json")
    }

    fn tailnet_log(&self) -> PathBuf {
        self.base.join("tailscale-commands.log")
    }

    #[cfg(unix)]
    fn install_fake_tailnet(&self) {
        let binary = self.base.join("tailscale");
        let state = self.tailnet_state();
        let log = self.tailnet_log();
        let pause = self.base.join("pause-tailnet-enable");
        let started = self.base.join("tailnet-enable-started");
        let pause_before = self.base.join("pause-before-tailnet-enable");
        let started_before = self.base.join("tailnet-enable-started-before-write");
        let occupy_before = self.base.join("occupy-before-tailnet-enable");
        let serve_status_seen = self.base.join("serve-status-seen");
        fs::write(
            &binary,
            format!(
                r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "{log}"
if [ "$1 $2" = "status --json" ]; then
  printf '%s' '{{"BackendState":"Running","Self":{{"Online":true,"DNSName":"device.example.ts.net."}}}}'
elif [ "$1 $2 $3" = "serve status --json" ]; then
  if [ -f "{occupy_before}" ] && [ ! -f "{serve_status_seen}" ]; then
    printf seen > "{serve_status_seen}"
    printf '{{}}'
  elif [ -f "{occupy_before}" ]; then
    if [ ! -f "{state}" ]; then
      printf '{{"TCP":{{"18443":{{"HTTPS":true}}}},"Web":{{"device.example.ts.net:18443":{{"Handlers":{{"/":{{"Proxy":"http://127.0.0.1:9/"}}}}}}}}}}' > "{state}"
    fi
    IFS= read -r value < "{state}" || true
    printf '%s' "$value"
  elif [ -f "{state}" ]; then
    IFS= read -r value < "{state}" || true
    printf '%s' "$value"
  else
    printf '{{}}'
  fi
elif [ "$1 $2" = "serve --bg" ]; then
  port="${{4#--https=}}"
  target="$5"
  if [ -f "{pause_before}" ]; then
    printf started > "{started_before}"
    while [ -f "{pause_before}" ]; do /bin/sleep 0.05; done
  fi
  printf '{{"TCP":{{"%s":{{"HTTPS":true}}}},"Web":{{"device.example.ts.net:%s":{{"Handlers":{{"/":{{"Proxy":"%s"}}}}}}}}}}' "$port" "$port" "$target" > "{state}"
  if [ -f "{pause}" ]; then
    printf started > "{started}"
    while [ -f "{pause}" ]; do /bin/sleep 0.05; done
  fi
elif [ "$1" = "serve" ] && [ "$3" = "off" ]; then
  printf '{{}}' > "{state}"
else
  exit 9
fi
"#,
                log = log.display(),
                state = state.display(),
                pause = pause.display(),
                started = started.display(),
                pause_before = pause_before.display(),
                started_before = started_before.display(),
                occupy_before = occupy_before.display(),
                serve_status_seen = serve_status_seen.display()
            ),
        )
        .unwrap();
        fs::set_permissions(binary, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn write_tailnet_state(&self, https_port: u16, target: &str) {
        fs::write(
            self.tailnet_state(),
            json!({
                "TCP": {https_port.to_string(): {"HTTPS": true}},
                "Web": {
                    format!("device.example.ts.net:{https_port}"): {
                        "Handlers": {"/": {"Proxy": target}}
                    }
                }
            })
            .to_string(),
        )
        .unwrap();
    }

    fn repository(&self, name: &str) -> PathBuf {
        let root = self.base.join(name);
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
        root
    }
}

#[cfg(unix)]
#[test]
fn tailnet_lifecycle_is_private_idempotent_and_exactly_owned() {
    let fixture = ServiceFixture::new(available_port());
    fixture.install_fake_tailnet();
    let https_port = 18_443;
    let first = fixture.json(&[
        "up",
        "--tailnet",
        "--tailnet-port",
        &https_port.to_string(),
        "--json",
    ]);
    assert_eq!(first["state"], "running");
    assert_eq!(first["tailnet"]["enabled"], true);
    assert_eq!(first["tailnet"]["verified"], true);
    assert_eq!(
        first["tailnet"]["origin"],
        "https://device.example.ts.net:18443"
    );
    let endpoint = first["endpoint"].as_str().unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));

    let config: Value = serde_json::from_slice(&fs::read(fixture.config()).unwrap()).unwrap();
    let token = config["token"].as_str().unwrap();
    let serialized = config.to_string();
    assert!(!serialized.contains("reviewer@example.com"));
    assert!(!serialized.contains("Tailscale-User"));
    let human = text(&fixture.run(&["status"], true));
    assert!(human.contains("Tailnet: verified"));
    assert!(human.contains("https://device.example.ts.net:18443"));
    assert!(!human.contains(token));
    assert!(!human.contains(fixture.base.to_str().unwrap()));

    let http = reqwest::blocking::Client::new();
    assert_eq!(
        http.get(endpoint)
            .header(reqwest::header::HOST, "device.example.ts.net:18443")
            .header("Tailscale-User-Login", "reviewer@example.com")
            .header("X-Forwarded-For", "100.64.0.1")
            .header("X-Forwarded-Host", "device.example.ts.net:18443")
            .send()
            .unwrap()
            .status(),
        reqwest::StatusCode::OK
    );
    assert_eq!(
        http.get(endpoint)
            .header(reqwest::header::HOST, "device.example.ts.net:18443")
            .send()
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(
        http.get(format!("{endpoint}__control/health"))
            .header(reqwest::header::HOST, "device.example.ts.net:18443")
            .header("Tailscale-User-Login", "reviewer@example.com")
            .header("X-Forwarded-For", "100.64.0.1")
            .header("X-Forwarded-Host", "device.example.ts.net:18443")
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );

    let repeated = fixture.json(&[
        "up",
        "--tailnet",
        "--tailnet-port",
        &https_port.to_string(),
        "--json",
    ]);
    assert_eq!(repeated["pid"], first["pid"]);
    let commands = fs::read_to_string(fixture.tailnet_log()).unwrap();
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.contains("--bg"))
            .count(),
        1
    );

    let repository = fixture.repository("tailnet-project");
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
                "tailnet-chat",
                "--label",
                "Test",
            ],
        )
        .output()
        .unwrap();
    assert!(attached.status.success(), "{}", text(&attached));
    let binding: Value = serde_json::from_slice(&attached.stdout).unwrap();
    assert!(
        binding["url"]
            .as_str()
            .unwrap()
            .starts_with("https://device.example.ts.net:18443/w/")
    );
    assert!(
        binding["local_url"]
            .as_str()
            .unwrap()
            .starts_with("http://127.0.0.1:")
    );
    assert_eq!(
        binding["tailnet_origin"],
        "https://device.example.ts.net:18443"
    );
    assert!(!binding.to_string().contains(token));
    assert!(!binding.to_string().contains("reviewer@example.com"));
    let remote_board = http
        .get(format!(
            "{endpoint}api/workspaces/{}/reviews/{}",
            binding["workspace_id"].as_str().unwrap(),
            binding["review_id"].as_str().unwrap()
        ))
        .header(reqwest::header::HOST, "device.example.ts.net:18443")
        .header("Tailscale-User-Login", "reviewer@example.com")
        .header("X-Forwarded-For", "100.64.0.1")
        .header("X-Forwarded-Host", "device.example.ts.net:18443")
        .send()
        .unwrap();
    assert_eq!(remote_board.status(), reqwest::StatusCode::OK);
    let remote_comment = http
        .post(format!(
            "{endpoint}api/workspaces/{}/reviews/{}/general",
            binding["workspace_id"].as_str().unwrap(),
            binding["review_id"].as_str().unwrap()
        ))
        .header(reqwest::header::HOST, "device.example.ts.net:18443")
        .header(
            reqwest::header::ORIGIN,
            "https://device.example.ts.net:18443",
        )
        .header("Tailscale-User-Login", "reviewer@example.com")
        .header("X-Forwarded-For", "100.64.0.1")
        .header("X-Forwarded-Host", "device.example.ts.net:18443")
        .json(&json!({"body": "Remote review comment."}))
        .send()
        .unwrap();
    assert_eq!(remote_comment.status(), reqwest::StatusCode::CREATED);
    let tailscale_before_poll = fs::read_to_string(fixture.tailnet_log()).unwrap();
    let polled = fixture
        .command(
            &repository,
            &[
                "connector",
                "poll",
                "--connection",
                binding["connection_id"].as_str().unwrap(),
                "--wait",
                "0",
            ],
        )
        .output()
        .unwrap();
    assert!(polled.status.success(), "{}", text(&polled));
    assert_eq!(
        fs::read_to_string(fixture.tailnet_log()).unwrap(),
        tailscale_before_poll
    );
    let registry = fs::read(fixture.base.join("data/truespec/registry.sqlite3")).unwrap();
    assert!(!String::from_utf8_lossy(&registry).contains("reviewer@example.com"));
    let service_log = fs::read_to_string(fixture.log()).unwrap();
    assert!(!service_log.contains("device.example.ts.net"));
    assert!(!service_log.contains("reviewer@example.com"));

    let first_pid = first["pid"].as_u64().unwrap();
    let shutdown = http
        .post(format!("{endpoint}__control/shutdown"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .unwrap();
    assert_eq!(shutdown.status(), reqwest::StatusCode::ACCEPTED);
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture.json(&["status", "--json"])["state"] != "stopped" {
        assert!(
            Instant::now() < deadline,
            "service did not stop for recovery test"
        );
        thread::sleep(Duration::from_millis(50));
    }
    let tailscale = fixture.base.join("tailscale");
    let unavailable_tailscale = fixture.base.join("tailscale-unavailable");
    fs::rename(&tailscale, &unavailable_tailscale).unwrap();
    let cold_poll = fixture
        .command(
            &repository,
            &[
                "connector",
                "poll",
                "--connection",
                binding["connection_id"].as_str().unwrap(),
                "--wait",
                "0",
            ],
        )
        .output()
        .unwrap();
    assert!(cold_poll.status.success(), "{}", text(&cold_poll));
    let locally_recovered = fixture.json(&["status", "--json"]);
    assert_eq!(locally_recovered["state"], "running");
    assert_eq!(locally_recovered["tailnet"]["verified"], false);
    fs::rename(&unavailable_tailscale, &tailscale).unwrap();
    let recovered = fixture
        .command(
            &repository,
            &[
                "connector",
                "attach",
                "--kind",
                "test",
                "--destination",
                "tailnet-chat",
                "--label",
                "Test",
            ],
        )
        .output()
        .unwrap();
    assert!(recovered.status.success(), "{}", text(&recovered));
    let recovered: Value = serde_json::from_slice(&recovered.stdout).unwrap();
    assert!(
        recovered["url"]
            .as_str()
            .unwrap()
            .starts_with("https://device.example.ts.net:18443/w/")
    );
    let recovered_status = fixture.json(&["status", "--json"]);
    assert_eq!(recovered_status["tailnet"]["verified"], true);
    assert_ne!(recovered_status["pid"].as_u64().unwrap(), first_pid);

    let before_reset_pid = recovered_status["pid"].as_u64().unwrap();
    let reset = fixture
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
                "--force",
                "--reset-state",
            ],
        )
        .output()
        .unwrap();
    assert!(reset.status.success(), "{}", text(&reset));
    let after_reset = fixture.json(&["status", "--json"]);
    assert_eq!(after_reset["tailnet"]["enabled"], true);
    assert_eq!(after_reset["tailnet"]["verified"], true);
    assert_ne!(after_reset["pid"].as_u64().unwrap(), before_reset_pid);

    fixture.write_tailnet_state(https_port, "http://127.0.0.1:9/");
    let refused = fixture.run(&["down", "--json"], false);
    assert!(text(&refused).contains("no longer matches TrueSpec"));
    assert!(text(&refused).contains("local TrueSpec service was stopped"));
    assert_eq!(fixture.json(&["status", "--json"])["state"], "stopped");
    fixture.write_tailnet_state(https_port, endpoint);
    let stopped = fixture.json(&["down", "--json"]);
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["tailnet"]["enabled"], false);
    assert_eq!(fs::read_to_string(fixture.tailnet_state()).unwrap(), "{}");
}

#[cfg(unix)]
#[test]
fn tailnet_start_refuses_an_occupied_serve_port_without_mutating_it() {
    let fixture = ServiceFixture::new(available_port());
    fixture.install_fake_tailnet();
    let invalid = fixture.run(&["up", "--tailnet", "--tailnet-port", "0", "--json"], false);
    assert!(text(&invalid).contains("between 1 and 65535"));
    fixture.write_tailnet_state(18_443, "http://127.0.0.1:9/");
    let before = fs::read_to_string(fixture.tailnet_state()).unwrap();
    let refused = fixture.run(
        &["up", "--tailnet", "--tailnet-port", "18443", "--json"],
        false,
    );
    assert!(text(&refused).contains("already configured"));
    assert_eq!(fs::read_to_string(fixture.tailnet_state()).unwrap(), before);
    let config: Value = serde_json::from_slice(&fs::read(fixture.config()).unwrap()).unwrap();
    assert_eq!(config["tailnet"]["enabled"], false);
}

#[cfg(unix)]
#[test]
fn concurrent_tailnet_transitions_publish_and_own_only_one_listener() {
    let fixture = ServiceFixture::new(available_port());
    fixture.install_fake_tailnet();
    let first = fixture
        .command(
            &fixture.base,
            &["up", "--tailnet", "--tailnet-port", "18443", "--json"],
        )
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let second = fixture
        .command(
            &fixture.base,
            &["up", "--tailnet", "--tailnet-port", "18444", "--json"],
        )
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let outputs = [
        first.wait_with_output().unwrap(),
        second.wait_with_output().unwrap(),
    ];
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1,
        "{}\n{}",
        text(&outputs[0]),
        text(&outputs[1])
    );
    let commands = fs::read_to_string(fixture.tailnet_log()).unwrap();
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.contains("--bg"))
            .count(),
        1
    );
    let status = fixture.json(&["status", "--json"]);
    assert_eq!(status["tailnet"]["enabled"], true);
    assert_eq!(status["tailnet"]["verified"], true);
    assert_eq!(fixture.json(&["down", "--json"])["state"], "stopped");
    assert_eq!(fs::read_to_string(fixture.tailnet_state()).unwrap(), "{}");
}

#[cfg(unix)]
#[test]
fn interrupted_tailnet_enable_remains_exactly_recoverable() {
    let fixture = ServiceFixture::new(available_port());
    fixture.install_fake_tailnet();
    let pause = fixture.base.join("pause-tailnet-enable");
    let started = fixture.base.join("tailnet-enable-started");
    fs::write(&pause, "pause").unwrap();
    let mut command = fixture.command(
        &fixture.base,
        &["up", "--tailnet", "--tailnet-port", "18443", "--json"],
    );
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !started.is_file() {
        assert!(
            Instant::now() < deadline,
            "tailnet enable did not reach the interruption point"
        );
        thread::sleep(Duration::from_millis(25));
    }
    child.kill().unwrap();
    let _ = child.wait();
    fs::remove_file(&pause).unwrap();
    thread::sleep(Duration::from_millis(100));

    let config: Value = serde_json::from_slice(&fs::read(fixture.config()).unwrap()).unwrap();
    assert_eq!(config["tailnet"]["enabled"], true);
    assert_eq!(config["tailnet"]["https_port"], 18_443);
    let stopped = fixture.json(&["down", "--json"]);
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["tailnet"]["enabled"], false);
    assert_eq!(fs::read_to_string(fixture.tailnet_state()).unwrap(), "{}");
}

#[cfg(unix)]
#[test]
fn cleanup_waits_for_a_surviving_prepublication_tailscale_child() {
    let fixture = ServiceFixture::new(available_port());
    fixture.install_fake_tailnet();
    let pause = fixture.base.join("pause-before-tailnet-enable");
    let started = fixture.base.join("tailnet-enable-started-before-write");
    fs::write(&pause, "pause").unwrap();
    let mut enable = fixture
        .command(
            &fixture.base,
            &["up", "--tailnet", "--tailnet-port", "18443", "--json"],
        )
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !started.is_file() {
        assert!(
            Instant::now() < deadline,
            "tailnet enable did not reach the prepublication interruption point"
        );
        thread::sleep(Duration::from_millis(25));
    }
    enable.kill().unwrap();
    let _ = enable.wait();
    assert!(!fixture.tailnet_state().exists());

    let mut down = fixture
        .command(&fixture.base, &["down", "--json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    assert!(
        down.try_wait().unwrap().is_none(),
        "cleanup passed the surviving Tailscale child"
    );
    fs::remove_file(&pause).unwrap();
    let output = down.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["state"], "stopped");
    assert_eq!(status["tailnet"]["enabled"], false);
    assert_eq!(fs::read_to_string(fixture.tailnet_state()).unwrap(), "{}");
}

#[cfg(unix)]
#[test]
fn external_serve_race_restores_local_policy_without_touching_foreign_state() {
    let fixture = ServiceFixture::new(available_port());
    fixture.install_fake_tailnet();
    fs::write(fixture.base.join("occupy-before-tailnet-enable"), "race").unwrap();
    let failed = fixture.run(
        &["up", "--tailnet", "--tailnet-port", "18443", "--json"],
        false,
    );
    assert!(
        text(&failed).contains("changed ownership during setup"),
        "{}",
        text(&failed)
    );
    let config: Value = serde_json::from_slice(&fs::read(fixture.config()).unwrap()).unwrap();
    assert_eq!(config["tailnet"]["enabled"], false);
    assert_eq!(fixture.json(&["status", "--json"])["state"], "stopped");
    let foreign = fs::read_to_string(fixture.tailnet_state()).unwrap();
    assert!(foreign.contains("http://127.0.0.1:9/"));
    let commands = fs::read_to_string(fixture.tailnet_log()).unwrap();
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.contains("--bg"))
            .count(),
        0
    );
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
