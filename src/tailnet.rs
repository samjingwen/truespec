use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

const OUTPUT_LIMIT: u64 = 256 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TailnetDevice {
    pub dns_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServePort {
    Free,
    Matching,
    Occupied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishResult {
    Published,
    BecameOccupied,
}

#[derive(Debug)]
pub struct TailnetClient {
    binary: OsString,
    timeout: Duration,
    test_binary: bool,
    operation_guard: Option<File>,
}

impl TailnetClient {
    pub fn discover() -> Self {
        let override_binary = std::env::var_os("TRUESPEC_TAILSCALE_BIN");
        Self {
            test_binary: cfg!(debug_assertions) && override_binary.is_some(),
            binary: override_binary.unwrap_or_else(|| OsString::from("tailscale")),
            timeout: COMMAND_TIMEOUT,
            operation_guard: None,
        }
    }

    pub fn with_operation_guard(mut self, guard: &File) -> Result<Self> {
        self.operation_guard = Some(
            guard
                .try_clone()
                .context("Cannot inherit the TrueSpec lifecycle lock")?,
        );
        Ok(self)
    }

    pub fn device(&self) -> Result<TailnetDevice> {
        let output = self.run(["status", "--json", "--peers=false"])?;
        let value: Value =
            serde_json::from_slice(&output).context("Tailscale status returned invalid JSON")?;
        ensure!(
            value.get("BackendState").and_then(Value::as_str) == Some("Running"),
            "Tailscale is not connected"
        );
        let local = value
            .get("Self")
            .and_then(Value::as_object)
            .context("Tailscale status does not include the local device")?;
        ensure!(
            local.get("Online").and_then(Value::as_bool) != Some(false),
            "The local Tailscale device is offline"
        );
        let dns_name = local
            .get("DNSName")
            .and_then(Value::as_str)
            .context("Tailscale status does not include a device DNS name")?
            .trim_end_matches('.')
            .to_ascii_lowercase();
        validate_dns_name(&dns_name)?;
        Ok(TailnetDevice { dns_name })
    }

    pub fn serve_port(&self, dns_name: &str, port: u16, target: &str) -> Result<ServePort> {
        validate_dns_name(dns_name)?;
        let output = self.run(["serve", "status", "--json"])?;
        let value: Value = serde_json::from_slice(&output)
            .context("Tailscale Serve status returned invalid JSON")?;
        parse_serve_port(&value, dns_name, port, target)
    }

    pub fn enable(&self, dns_name: &str, port: u16, target: &str) -> Result<()> {
        match self.serve_port(dns_name, port, target)? {
            ServePort::Matching => return Ok(()),
            ServePort::Occupied => {
                bail!("Tailscale Serve HTTPS port {port} is already in use")
            }
            ServePort::Free => {}
        }
        self.configure(port, target)?;
        ensure!(
            self.serve_port(dns_name, port, target)? == ServePort::Matching,
            "Tailscale Serve did not publish the expected HTTPS proxy"
        );
        Ok(())
    }

    pub fn publish_if_free(
        &self,
        dns_name: &str,
        port: u16,
        target: &str,
    ) -> Result<PublishResult> {
        match self.serve_port(dns_name, port, target)? {
            ServePort::Free => {}
            ServePort::Matching | ServePort::Occupied => {
                return Ok(PublishResult::BecameOccupied);
            }
        }
        self.configure(port, target)?;
        ensure!(
            self.serve_port(dns_name, port, target)? == ServePort::Matching,
            "Tailscale Serve did not publish the expected HTTPS proxy"
        );
        Ok(PublishResult::Published)
    }

    pub fn disable(&self, dns_name: &str, port: u16, target: &str) -> Result<()> {
        match self.serve_port(dns_name, port, target)? {
            ServePort::Free => return Ok(()),
            ServePort::Occupied => bail!(
                "Tailscale Serve HTTPS port {port} no longer matches TrueSpec; run `tailscale serve --https={port} off` only after inspecting it"
            ),
            ServePort::Matching => {}
        }
        self.run([
            OsStr::new("serve"),
            OsString::from(format!("--https={port}")).as_os_str(),
            OsStr::new("off"),
        ])
        .with_context(|| {
            format!(
                "Cannot remove the owned Serve listener; inspect port {port} before running `tailscale serve --https={port} off`"
            )
        })?;
        ensure!(
            self.serve_port(dns_name, port, target)? == ServePort::Free,
            "Tailscale Serve did not remove the expected HTTPS proxy; inspect port {port} before running `tailscale serve --https={port} off`"
        );
        Ok(())
    }

    pub fn origin(device: &TailnetDevice, port: u16) -> String {
        if port == 443 {
            format!("https://{}", device.dns_name)
        } else {
            format!("https://{}:{port}", device.dns_name)
        }
    }

    pub fn verify_browser(&self, origin: &str) -> Result<()> {
        if self.test_binary {
            return Ok(());
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("Cannot build the tailnet readiness client")?;
        let deadline = Instant::now() + self.timeout;
        loop {
            if client
                .get(format!("{origin}/"))
                .send()
                .is_ok_and(|response| response.status().is_success())
            {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "Tailnet browser origin did not become reachable with an identified request"
            );
            thread::sleep(Duration::from_millis(250));
        }
    }

    fn configure(&self, port: u16, target: &str) -> Result<()> {
        self.run([
            OsStr::new("serve"),
            OsStr::new("--bg"),
            OsStr::new("--yes"),
            OsString::from(format!("--https={port}")).as_os_str(),
            OsStr::new(target),
        ])
        .context(
            "Cannot configure Tailscale Serve; confirm HTTPS certificates are enabled for the tailnet",
        )?;
        Ok(())
    }

    fn run<I, S>(&self, args: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.binary);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        if let Some(guard) = &self.operation_guard {
            use std::os::{fd::AsRawFd, unix::process::CommandExt};

            let descriptor = guard.as_raw_fd();
            // The child keeps the lifecycle flock across exec, so another
            // TrueSpec process cannot reconcile while this mutation survives.
            unsafe {
                command.pre_exec(move || {
                    let flags = nix::libc::fcntl(descriptor, nix::libc::F_GETFD);
                    if flags < 0
                        || nix::libc::fcntl(
                            descriptor,
                            nix::libc::F_SETFD,
                            flags & !nix::libc::FD_CLOEXEC,
                        ) < 0
                    {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let mut child = command.spawn().context("Cannot run the Tailscale CLI")?;
        let stdout = child
            .stdout
            .take()
            .context("Cannot read Tailscale output")?;
        let stderr = child
            .stderr
            .take()
            .context("Cannot read Tailscale errors")?;
        let stdout = read_bounded(stdout);
        let stderr = read_bounded(stderr);
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                bail!("Tailscale CLI timed out");
            }
            thread::sleep(Duration::from_millis(25));
        };
        let stdout = stdout
            .join()
            .map_err(|_| anyhow::anyhow!("Cannot join Tailscale output reader"))??;
        let stderr = stderr
            .join()
            .map_err(|_| anyhow::anyhow!("Cannot join Tailscale error reader"))??;
        ensure!(
            stdout.len() as u64 <= OUTPUT_LIMIT && stderr.len() as u64 <= OUTPUT_LIMIT,
            "Tailscale CLI output exceeded 256 KiB"
        );
        ensure!(status.success(), "Tailscale CLI command failed");
        Ok(stdout)
    }
}

fn read_bounded(input: impl Read + Send + 'static) -> thread::JoinHandle<Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        input.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn validate_dns_name(value: &str) -> Result<()> {
    let labels_valid = value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
    });
    ensure!(
        !value.is_empty() && value.len() <= 253 && value.ends_with(".ts.net") && labels_valid,
        "Tailscale returned an invalid device DNS name"
    );
    Ok(())
}

fn parse_serve_port(value: &Value, dns_name: &str, port: u16, target: &str) -> Result<ServePort> {
    let object = value
        .as_object()
        .context("Tailscale Serve status must be a JSON object")?;
    for key in object.keys() {
        ensure!(
            matches!(
                key.as_str(),
                "TCP" | "Web" | "AllowFunnel" | "Foreground" | "Services"
            ),
            "Tailscale Serve status uses an unsupported schema"
        );
    }
    let port_key = port.to_string();
    let tcp = object.get("TCP").and_then(Value::as_object);
    let tcp_entry = tcp.and_then(|tcp| tcp.get(&port_key));
    let host_port = format!("{dns_name}:{port}");
    let web = object.get("Web").and_then(Value::as_object);
    let web_entry = web.and_then(|web| web.get(&host_port));
    let funnel_entry = object
        .get("AllowFunnel")
        .and_then(Value::as_object)
        .and_then(|entries| entries.get(&host_port));
    if tcp_entry.is_none() && web_entry.is_none() && funnel_entry.is_none() {
        return Ok(ServePort::Free);
    }
    let https = tcp_entry.and_then(Value::as_object).is_some_and(|entry| {
        entry.len() == 1 && entry.get("HTTPS").and_then(Value::as_bool) == Some(true)
    });
    let proxy = web_entry.and_then(Value::as_object).and_then(|entry| {
        if entry.len() != 1 {
            return None;
        }
        let handlers = entry.get("Handlers")?.as_object()?;
        if handlers.len() != 1 {
            return None;
        }
        let handler = handlers.get("/")?.as_object()?;
        if handler.len() != 1 {
            return None;
        }
        handler.get("Proxy")?.as_str()
    });
    let publicly_enabled = funnel_entry.and_then(Value::as_bool) == Some(true);
    Ok(
        if https
            && !publicly_enabled
            && proxy.is_some_and(|proxy| proxy_targets_match(proxy, target))
        {
            ServePort::Matching
        } else {
            ServePort::Occupied
        },
    )
}

fn proxy_targets_match(left: &str, right: &str) -> bool {
    let parse = |value: &str| {
        let url = reqwest::Url::parse(value).ok()?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return None;
        }
        Some((url.host_str()?.to_owned(), url.port()?))
    };
    parse(left).is_some_and(|left| Some(left) == parse(right))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };

    use super::*;

    fn script(root: &Path, body: &str) -> TailnetClient {
        let path = root.join("tailscale");
        fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        TailnetClient {
            binary: path.into_os_string(),
            timeout: Duration::from_secs(3),
            test_binary: true,
            operation_guard: None,
        }
    }

    #[test]
    fn discovers_connected_device_and_origin() {
        let fixture = tempfile::tempdir().unwrap();
        let client = script(
            fixture.path(),
            r#"printf '%s' '{"BackendState":"Running","Self":{"Online":true,"DNSName":"device.example.ts.net."}}'"#,
        );
        let device = client.device().unwrap();
        assert_eq!(device.dns_name, "device.example.ts.net");
        assert_eq!(
            TailnetClient::origin(&device, 8443),
            "https://device.example.ts.net:8443"
        );
    }

    #[test]
    fn refuses_missing_disconnected_malformed_and_oversized_cli() {
        let missing = TailnetClient {
            binary: PathBuf::from("/definitely/missing/tailscale").into_os_string(),
            timeout: Duration::from_millis(50),
            test_binary: true,
            operation_guard: None,
        };
        assert!(missing.device().is_err());
        let fixture = tempfile::tempdir().unwrap();
        assert!(
            script(
                fixture.path(),
                r#"printf '%s' '{"BackendState":"Stopped","Self":{}}'"#
            )
            .device()
            .is_err()
        );
        assert!(
            script(fixture.path(), "printf 'not-json'")
                .device()
                .is_err()
        );
        assert!(
            script(
                fixture.path(),
                "dd if=/dev/zero bs=1024 count=300 2>/dev/null"
            )
            .device()
            .is_err()
        );
    }

    #[test]
    fn enforces_timeout_and_sanitizes_failure_prefix() {
        let fixture = tempfile::tempdir().unwrap();
        let mut slow = script(fixture.path(), "sleep 2");
        slow.timeout = Duration::from_millis(100);
        assert!(slow.device().is_err());
        let error = script(
            fixture.path(),
            "printf 'private failure detail' >&2; exit 1",
        )
        .device()
        .unwrap_err()
        .to_string();
        assert_eq!(error, "Tailscale CLI command failed");
        assert!(!error.contains("private failure detail"));
    }

    #[test]
    fn parses_free_matching_occupied_and_unknown_serve_state() {
        let free = serde_json::json!({});
        assert_eq!(
            parse_serve_port(
                &free,
                "device.example.ts.net",
                8443,
                "http://127.0.0.1:47371/"
            )
            .unwrap(),
            ServePort::Free
        );
        let matching = serde_json::json!({
            "TCP": {"8443": {"HTTPS": true}},
            "Web": {
                "device.example.ts.net:8443": {
                    "Handlers": {"/": {"Proxy": "http://127.0.0.1:47371/"}}
                }
            }
        });
        assert_eq!(
            parse_serve_port(
                &matching,
                "device.example.ts.net",
                8443,
                "http://127.0.0.1:47371/"
            )
            .unwrap(),
            ServePort::Matching
        );
        let normalized = serde_json::json!({
            "TCP": {"8443": {"HTTPS": true}},
            "Web": {
                "device.example.ts.net:8443": {
                    "Handlers": {"/": {"Proxy": "http://127.0.0.1:47371"}}
                }
            }
        });
        assert_eq!(
            parse_serve_port(
                &normalized,
                "device.example.ts.net",
                8443,
                "http://127.0.0.1:47371/"
            )
            .unwrap(),
            ServePort::Matching
        );
        let occupied = serde_json::json!({"TCP": {"8443": {"TCPForward": "127.0.0.1:22"}}});
        assert_eq!(
            parse_serve_port(
                &occupied,
                "device.example.ts.net",
                8443,
                "http://127.0.0.1:47371/"
            )
            .unwrap(),
            ServePort::Occupied
        );
        let extra_path = serde_json::json!({
            "TCP": {"8443": {"HTTPS": true}},
            "Web": {
                "device.example.ts.net:8443": {
                    "Handlers": {
                        "/": {"Proxy": "http://127.0.0.1:47371/"},
                        "/other": {"Proxy": "http://127.0.0.1:9/"}
                    }
                }
            }
        });
        assert_eq!(
            parse_serve_port(
                &extra_path,
                "device.example.ts.net",
                8443,
                "http://127.0.0.1:47371/"
            )
            .unwrap(),
            ServePort::Occupied
        );
        let publicly_enabled = serde_json::json!({
            "TCP": {"8443": {"HTTPS": true}},
            "Web": {
                "device.example.ts.net:8443": {
                    "Handlers": {"/": {"Proxy": "http://127.0.0.1:47371/"}}
                }
            },
            "AllowFunnel": {"device.example.ts.net:8443": true}
        });
        assert_eq!(
            parse_serve_port(
                &publicly_enabled,
                "device.example.ts.net",
                8443,
                "http://127.0.0.1:47371/"
            )
            .unwrap(),
            ServePort::Occupied
        );
        assert!(
            parse_serve_port(
                &serde_json::json!({"NewField": true}),
                "device.example.ts.net",
                8443,
                "http://127.0.0.1:47371/"
            )
            .is_err()
        );
    }

    #[test]
    fn fake_cli_covers_enable_disable_and_failure() {
        let fixture = tempfile::tempdir().unwrap();
        let state = fixture.path().join("state");
        let body = format!(
            r#"
if [ "$1 $2" = "status --json" ]; then
  printf '%s' '{{"BackendState":"Running","Self":{{"Online":true,"DNSName":"device.example.ts.net."}}}}'
elif [ "$1 $2 $3" = "serve status --json" ]; then
  if [ -f "{state}" ]; then
    cat "{state}"
  else
    printf '{{}}'
  fi
elif [ "$1" = "serve" ] && [ "$2" = "--bg" ]; then
  printf '%s' '{{"TCP":{{"8443":{{"HTTPS":true}}}},"Web":{{"device.example.ts.net:8443":{{"Handlers":{{"/":{{"Proxy":"http://127.0.0.1:47371/"}}}}}}}}}}' > "{state}"
elif [ "$1" = "serve" ] && [ "$3" = "off" ]; then
  printf '{{}}' > "{state}"
else
  exit 9
fi
"#,
            state = state.display()
        );
        let client = script(fixture.path(), &body);
        client
            .enable("device.example.ts.net", 8443, "http://127.0.0.1:47371/")
            .unwrap();
        assert_eq!(
            client
                .serve_port("device.example.ts.net", 8443, "http://127.0.0.1:47371/")
                .unwrap(),
            ServePort::Matching
        );
        client
            .disable("device.example.ts.net", 8443, "http://127.0.0.1:47371/")
            .unwrap();
        assert_eq!(
            client
                .serve_port("device.example.ts.net", 8443, "http://127.0.0.1:47371/")
                .unwrap(),
            ServePort::Free
        );
    }
}
