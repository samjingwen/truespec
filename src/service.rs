use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use reqwest::{
    StatusCode,
    blocking::{Client, RequestBuilder},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    service_registry::ProjectRegistry,
    tailnet::{PublishResult, ServePort, TailnetClient},
};

pub const SERVICE_PROTOCOL: u8 = 2;
pub const DEFAULT_PORT: u16 = 47_371;
const FILE_LIMIT: u64 = 64 * 1024;
const LOG_LIMIT: u64 = 1024 * 1024;
const CONFIG_VERSION: u8 = 2;
pub const DEFAULT_TAILNET_PORT: u16 = 8443;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const START_TIMEOUT: Duration = Duration::from_secs(12);
const STOP_TIMEOUT: Duration = Duration::from_secs(8);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
enum StartupPhase {
    Paths,
    Ownership,
    Configuration,
    Registry,
    Runtime,
    EndpointBind,
    Registration,
    Logging,
    Serving,
}

impl StartupPhase {
    fn label(&self) -> &'static str {
        match self {
            Self::Paths => "paths",
            Self::Ownership => "ownership",
            Self::Configuration => "configuration",
            Self::Registry => "registry",
            Self::Runtime => "runtime",
            Self::EndpointBind => "endpoint_bind",
            Self::Registration => "registration",
            Self::Logging => "logging",
            Self::Serving => "serving",
        }
    }
}

impl std::fmt::Display for StartupPhase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

impl std::error::Error for StartupPhase {}

#[derive(Clone, Debug)]
pub struct ServicePaths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl ServicePaths {
    pub fn resolve() -> Result<Self> {
        Self::from_bases(
            std::env::var_os("HOME").map(PathBuf::from),
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            std::env::var_os("XDG_STATE_HOME").map(PathBuf::from),
            std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        )
    }

    fn from_bases(
        home: Option<PathBuf>,
        config: Option<PathBuf>,
        state: Option<PathBuf>,
        data: Option<PathBuf>,
    ) -> Result<Self> {
        let config = config.filter(|path| path.is_absolute());
        let state = state.filter(|path| path.is_absolute());
        let data = data.filter(|path| path.is_absolute());
        let needs_home = config.is_none() || state.is_none() || data.is_none();
        let home = if needs_home {
            let home = home.ok_or_else(|| {
                anyhow::anyhow!("Cannot resolve the current user's home directory")
            })?;
            ensure!(
                home.is_absolute(),
                "The current user's home directory must be absolute"
            );
            Some(home)
        } else {
            None
        };
        let config = config.unwrap_or_else(|| home.as_ref().unwrap().join(".config"));
        let state = state.unwrap_or_else(|| home.as_ref().unwrap().join(".local/state"));
        let data = data.unwrap_or_else(|| home.as_ref().unwrap().join(".local/share"));
        Ok(Self {
            config_dir: config.join("truespec"),
            state_dir: state.join("truespec"),
            data_dir: data.join("truespec"),
        })
    }

    pub fn ensure(&self) -> Result<()> {
        for directory in [&self.config_dir, &self.state_dir, &self.data_dir] {
            secure_directory(directory)?;
        }
        Ok(())
    }

    pub fn config(&self) -> PathBuf {
        self.config_dir.join("service.json")
    }

    pub fn registration(&self) -> PathBuf {
        self.state_dir.join("service.json")
    }

    fn registration_lock(&self) -> PathBuf {
        self.state_dir.join("service-registration.lock")
    }

    pub fn lock(&self) -> PathBuf {
        self.state_dir.join("service.lock")
    }

    fn maintenance_lock(&self) -> PathBuf {
        self.state_dir.join("service-maintenance.lock")
    }

    fn lifecycle_lock(&self) -> PathBuf {
        self.state_dir.join("service-lifecycle.lock")
    }

    pub fn log(&self) -> PathBuf {
        self.state_dir.join("service.log")
    }

    fn log_lock(&self) -> PathBuf {
        self.state_dir.join("service-log.lock")
    }

    pub fn registry(&self) -> PathBuf {
        self.data_dir.join("registry.sqlite3")
    }
}

pub struct OwnerLock(File);

impl OwnerLock {
    pub fn acquire(paths: &ServicePaths) -> Result<Option<Self>> {
        paths.ensure()?;
        let file = secure_open(&paths.lock(), true, true)?;
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Some(Self(file))),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error).context("Cannot lock the TrueSpec service owner record"),
        }
    }

    fn is_held(paths: &ServicePaths) -> Result<bool> {
        paths.ensure()?;
        let file = secure_open(&paths.lock(), true, false)?;
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => {
                FileExt::unlock(&file)?;
                Ok(false)
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
            Err(error) => Err(error).context("Cannot inspect the TrueSpec service owner lock"),
        }
    }
}

pub struct MaintenanceLock(File);
struct LifecycleLock(File);

pub struct ResetOutcome {
    pub reset: bool,
    pub cleanup_incomplete: bool,
}

impl MaintenanceLock {
    pub fn acquire() -> Result<Self> {
        let paths = ServicePaths::resolve()?;
        paths.ensure()?;
        let file = secure_open(&paths.maintenance_lock(), true, false)?;
        FileExt::try_lock_exclusive(&file)
            .context("TrueSpec maintenance is already in progress")?;
        Ok(Self(file))
    }

    fn is_held(paths: &ServicePaths) -> Result<bool> {
        let file = secure_open(&paths.maintenance_lock(), true, false)?;
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => {
                FileExt::unlock(&file)?;
                Ok(false)
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
            Err(error) => Err(error).context("Cannot inspect TrueSpec maintenance ownership"),
        }
    }
}

impl LifecycleLock {
    fn acquire(paths: &ServicePaths) -> Result<Self> {
        paths.ensure()?;
        let file = secure_open(&paths.lifecycle_lock(), true, false)?;
        FileExt::lock_exclusive(&file).context("Cannot lock TrueSpec service lifecycle")?;
        Ok(Self(file))
    }

    fn operation_guard(&self) -> &File {
        &self.0
    }
}

impl Drop for MaintenanceLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

impl Drop for OwnerLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

impl Drop for LifecycleLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    version: u8,
    port: u16,
    token: String,
    tailnet: TailnetConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TailnetConfig {
    pub enabled: bool,
    pub https_port: u16,
    pub origin: Option<String>,
    pub target: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyServiceConfig {
    version: u8,
    port: u16,
    token: String,
}

impl ServiceConfig {
    pub fn load_or_create(paths: &ServicePaths) -> Result<Self> {
        paths.ensure()?;
        let path = paths.config();
        match read_json::<Value>(&path) {
            Ok(value) => {
                let version = value
                    .get("version")
                    .and_then(Value::as_u64)
                    .context("TrueSpec service configuration has no version")?;
                if version == 1 {
                    let legacy: LegacyServiceConfig = serde_json::from_value(value)?;
                    ensure!(legacy.version == 1, "Invalid legacy service configuration");
                    let config = Self {
                        version: CONFIG_VERSION,
                        port: legacy.port,
                        token: legacy.token,
                        tailnet: TailnetConfig::disabled(),
                    };
                    config.validate()?;
                    return Ok(config);
                }
                let config: Self = serde_json::from_value(value)?;
                config.validate()?;
                Ok(config)
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                let port = std::env::var("TRUESPEC_SERVICE_PORT")
                    .ok()
                    .map(|value| value.parse())
                    .transpose()
                    .context("TRUESPEC_SERVICE_PORT must be a valid port")?
                    .unwrap_or(DEFAULT_PORT);
                let mut credential = [0_u8; 32];
                getrandom::fill(&mut credential).map_err(|error| {
                    anyhow::anyhow!(
                        "Cannot generate the private TrueSpec service credential: {error}"
                    )
                })?;
                let config = Self {
                    version: CONFIG_VERSION,
                    port,
                    token: credential
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<Vec<_>>()
                        .join(""),
                    tailnet: TailnetConfig::disabled(),
                };
                config.validate()?;
                create_json(&path, &config)?;
                let published: Self = read_json(&path)?;
                published.validate()?;
                Ok(published)
            }
            Err(error) => Err(error).context("Cannot load TrueSpec service configuration"),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn tailnet(&self) -> &TailnetConfig {
        &self.tailnet
    }

    fn save_tailnet(&self, paths: &ServicePaths, tailnet: TailnetConfig) -> Result<Self> {
        let config = Self {
            tailnet,
            ..self.clone()
        };
        config.validate()?;
        write_json(&paths.config(), &config)?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == CONFIG_VERSION,
            "Unsupported TrueSpec service configuration version {}",
            self.version
        );
        ensure!(
            self.token.len() == 64 && self.token.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "Invalid TrueSpec service credential"
        );
        self.tailnet.validate()?;
        if self.tailnet.enabled {
            let target = reqwest::Url::parse(self.tailnet.target.as_deref().unwrap())?;
            ensure!(
                target.port() == Some(self.port),
                "Tailnet target must match the configured TrueSpec service port"
            );
        }
        Ok(())
    }
}

impl TailnetConfig {
    fn disabled() -> Self {
        Self {
            enabled: false,
            https_port: DEFAULT_TAILNET_PORT,
            origin: None,
            target: None,
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(self.https_port > 0, "Invalid tailnet HTTPS port");
        if !self.enabled {
            ensure!(
                self.origin.is_none() && self.target.is_none(),
                "Disabled tailnet mode must not publish an origin or target"
            );
            return Ok(());
        }
        let origin = self
            .origin
            .as_deref()
            .context("Enabled tailnet mode requires an HTTPS origin")?;
        let origin = reqwest::Url::parse(origin)?;
        ensure!(
            origin.scheme() == "https"
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none()
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.port_or_known_default() == Some(self.https_port)
                && origin
                    .host_str()
                    .is_some_and(|host| host.ends_with(".ts.net")),
            "Invalid tailnet HTTPS origin"
        );
        let target = self
            .target
            .as_deref()
            .context("Enabled tailnet mode requires a loopback target")?;
        let target = reqwest::Url::parse(target)?;
        ensure!(
            target.scheme() == "http"
                && target.host_str() == Some("127.0.0.1")
                && target.port().is_some()
                && target.path() == "/"
                && target.query().is_none()
                && target.fragment().is_none()
                && target.username().is_empty()
                && target.password().is_none(),
            "Invalid tailnet loopback target"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationState {
    Starting,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub protocol: u8,
    pub version: String,
    pub instance_id: String,
    pub generation: String,
    pub pid: u32,
    pub url: String,
    pub state: RegistrationState,
}

impl Registration {
    pub fn read(paths: &ServicePaths) -> Result<Option<Self>> {
        let path = paths.registration();
        match read_json(&path) {
            Ok(value) => {
                let registration: Self = value;
                registration.validate()?;
                Ok(Some(registration))
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(None)
            }
            Err(error) => Err(error).context("Cannot read TrueSpec service registration"),
        }
    }

    pub fn write(&self, paths: &ServicePaths) -> Result<()> {
        self.validate()?;
        let _lock = registration_lock(paths)?;
        write_json(&paths.registration(), self)
    }

    pub fn remove_if_current(&self, paths: &ServicePaths) -> Result<()> {
        let _lock = registration_lock(paths)?;
        if Self::read(paths)?
            .as_ref()
            .is_some_and(|current| self.same_instance(current))
        {
            match fs::remove_file(paths.registration()) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("Cannot remove service registration"),
            }
        }
        Ok(())
    }

    fn same_instance(&self, other: &Self) -> bool {
        self.protocol == other.protocol
            && self.version == other.version
            && self.instance_id == other.instance_id
            && self.generation == other.generation
            && self.pid == other.pid
            && self.url == other.url
    }

    fn validate(&self) -> Result<()> {
        ensure!(self.protocol > 0, "Invalid service protocol");
        ensure!(
            Uuid::parse_str(&self.instance_id).is_ok() && Uuid::parse_str(&self.generation).is_ok(),
            "Invalid service instance identity"
        );
        ensure!(self.pid > 0, "Invalid service process identity");
        ensure!(
            !self.version.trim().is_empty(),
            "Invalid service version identity"
        );
        let url =
            reqwest::Url::parse(&self.url).context("Service registration has an invalid URL")?;
        ensure!(
            url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port().is_some()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
                && url.username().is_empty()
                && url.password().is_none(),
            "Service registration must use IPv4 loopback"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Health {
    pub protocol: u8,
    pub version: String,
    pub instance_id: String,
    pub pid: u32,
    pub state: String,
    pub projects: u64,
    pub open_reviews: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects: Option<u64>,
    pub open_reviews: Option<u64>,
    pub tailnet: TailnetStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TailnetStatus {
    pub enabled: bool,
    pub https_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    pub verified: bool,
}

impl TailnetStatus {
    fn disabled(https_port: u16) -> Self {
        Self {
            enabled: false,
            https_port,
            origin: None,
            verified: false,
        }
    }
}

struct TailnetTransition<'a> {
    client: &'a TailnetClient,
    dns_name: &'a str,
    https_port: u16,
    target: &'a str,
}

pub enum Probe {
    Ready(Registration, Health),
    Starting(Registration),
    Publishing,
    Failed(Registration),
    Incompatible(Registration, Health),
    Unresponsive(Registration),
    TimedOut(Registration),
    Stale(Registration),
    EndpointCollision(Option<Registration>, String),
    Stopped,
}

pub struct ServiceClient {
    paths: ServicePaths,
    config: ServiceConfig,
    http: Client,
}

struct ContenderGuard {
    child: Option<Child>,
}

impl ContenderGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        self.child
            .as_mut()
            .context("TrueSpec service contender is unavailable")?
            .try_wait()
            .context("Cannot inspect TrueSpec service contender")
    }

    fn release(&mut self) -> Option<Child> {
        self.child.take()
    }
}

impl Drop for ContenderGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl ServiceClient {
    pub fn open() -> Result<Self> {
        Self::with_paths(ServicePaths::resolve()?)
    }

    pub fn with_paths(paths: ServicePaths) -> Result<Self> {
        let config = ServiceConfig::load_or_create(&paths)?;
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("Cannot build TrueSpec service client")?;
        Ok(Self {
            paths,
            config,
            http,
        })
    }

    pub fn probe(&self) -> Result<Probe> {
        let Some(mut registration) = Registration::read(&self.paths)? else {
            return Ok(if OwnerLock::is_held(&self.paths)? {
                Probe::Publishing
            } else if endpoint_occupied(self.config.port) {
                Probe::EndpointCollision(None, configured_endpoint(self.config.port))
            } else {
                Probe::Stopped
            });
        };
        if registration.state == RegistrationState::Failed {
            return Ok(Probe::Failed(registration));
        }
        let response = self
            .authorize(
                self.http
                    .get(format!("{}__control/health", registration.url)),
            )
            .send();
        let response = match response {
            Ok(response) => response,
            Err(error) if error.is_timeout() => return Ok(Probe::TimedOut(registration)),
            Err(_) if !process_alive(registration.pid) => return Ok(Probe::Stale(registration)),
            Err(_) if registration.state == RegistrationState::Starting => {
                return Ok(Probe::Starting(registration));
            }
            Err(_) => return Ok(Probe::Unresponsive(registration)),
        };
        if !response.status().is_success() {
            let endpoint = registration.url.clone();
            return Ok(Probe::EndpointCollision(Some(registration), endpoint));
        }
        let health: Health = match response.json() {
            Ok(health) => health,
            Err(_) => {
                let endpoint = registration.url.clone();
                return Ok(Probe::EndpointCollision(Some(registration), endpoint));
            }
        };
        let matches = health.protocol == registration.protocol
            && health.version == registration.version
            && health.instance_id == registration.instance_id
            && health.pid == registration.pid;
        if !matches {
            let endpoint = registration.url.clone();
            return Ok(Probe::EndpointCollision(Some(registration), endpoint));
        }
        if health.protocol != SERVICE_PROTOCOL || health.version != env!("CARGO_PKG_VERSION") {
            Ok(Probe::Incompatible(registration, health))
        } else if health.state == "starting" {
            Ok(Probe::Starting(registration))
        } else if health.state == "failed" {
            Ok(Probe::Failed(registration))
        } else if health.state != "ready" {
            Ok(Probe::Unresponsive(registration))
        } else {
            registration.state = RegistrationState::Ready;
            Ok(Probe::Ready(registration, health))
        }
    }

    pub fn status(&self) -> Result<Status> {
        let mut status = match self.probe()? {
            Probe::Ready(registration, health) => {
                Status {
                    state: "running",
                    version: Some(health.version),
                    pid: Some(health.pid),
                    endpoint: Some(registration.url),
                    projects: Some(health.projects),
                    open_reviews: health.open_reviews,
                    tailnet: TailnetStatus::disabled(self.config.tailnet.https_port),
                    detail: None,
                }
            }
            Probe::Starting(registration) => status_from_registration(
                "starting",
                registration,
                "The service owner has not reported authenticated readiness yet.",
            ),
            Probe::Publishing => Status {
                state: "starting",
                version: None,
                pid: None,
                endpoint: None,
                projects: None,
                open_reviews: None,
                tailnet: TailnetStatus::disabled(self.config.tailnet.https_port),
                detail: Some(
                    "A TrueSpec owner is publishing authenticated service registration.".into(),
                ),
            },
            Probe::Failed(registration) => status_from_registration(
                "failed",
                registration,
                "The last service owner failed during startup. Run `truespec up` to retry.",
            ),
            Probe::Incompatible(registration, health) => Status {
                state: "incompatible",
                version: Some(health.version),
                pid: Some(health.pid),
                endpoint: Some(registration.url),
                projects: Some(health.projects),
                open_reviews: health.open_reviews,
                tailnet: TailnetStatus::disabled(self.config.tailnet.https_port),
                detail: Some("Run `truespec down` with the matching binary, then `truespec up`.".into()),
            },
            Probe::Unresponsive(registration) => Status {
                state: "unresponsive",
                version: Some(registration.version),
                pid: Some(registration.pid),
                endpoint: Some(registration.url),
                projects: None,
                open_reviews: None,
                tailnet: TailnetStatus::disabled(self.config.tailnet.https_port),
                detail: Some("The registered service did not answer authenticated health checks.".into()),
            },
            Probe::TimedOut(registration) => status_from_registration(
                "timeout",
                registration,
                "The registered service timed out during its authenticated health check.",
            ),
            Probe::Stale(registration) => status_from_registration(
                "stale",
                registration,
                "The registered service process is no longer running.",
            ),
            Probe::EndpointCollision(registration, endpoint) => Status {
                state: "endpoint_collision",
                version: registration.as_ref().map(|record| record.version.clone()),
                pid: registration.as_ref().map(|record| record.pid),
                endpoint: Some(endpoint),
                projects: None,
                open_reviews: None,
                tailnet: TailnetStatus::disabled(self.config.tailnet.https_port),
                detail: Some(
                    "The configured endpoint is occupied by an unverified process. TrueSpec left it untouched."
                        .into(),
                ),
            },
            Probe::Stopped => Status {
                state: "stopped",
                version: None,
                pid: None,
                endpoint: None,
                projects: Some(0),
                open_reviews: Some(0),
                tailnet: TailnetStatus::disabled(self.config.tailnet.https_port),
                detail: None,
            },
        };
        status.tailnet = self.tailnet_status();
        Ok(status)
    }

    pub fn up(&mut self) -> Result<Status> {
        let _lifecycle = LifecycleLock::acquire(&self.paths)?;
        self.config = ServiceConfig::load_or_create(&self.paths)?;
        drop(self.up_local()?);
        self.reconcile_tailnet(&_lifecycle)?;
        self.status()
    }

    pub fn ensure_up(&mut self) -> Result<()> {
        if matches!(self.probe()?, Probe::Ready(_, _)) {
            return Ok(());
        }
        let _lifecycle = LifecycleLock::acquire(&self.paths)?;
        self.config = ServiceConfig::load_or_create(&self.paths)?;
        self.up_local().map(drop)
    }

    pub fn up_tailnet(&mut self, https_port: u16) -> Result<Status> {
        let _lifecycle = LifecycleLock::acquire(&self.paths)?;
        self.config = ServiceConfig::load_or_create(&self.paths)?;
        ensure!(
            https_port > 0,
            "Tailnet HTTPS port must be between 1 and 65535"
        );
        ensure!(
            self.config.port > 0,
            "Tailnet mode requires a fixed TrueSpec service port"
        );
        if self.config.tailnet.enabled {
            ensure!(
                self.config.tailnet.https_port == https_port,
                "TrueSpec already owns tailnet HTTPS port {}; run `truespec down` before choosing another port",
                self.config.tailnet.https_port
            );
            drop(self.up_local()?);
            self.reconcile_tailnet(&_lifecycle)?;
            return self.status();
        }

        let tailnet =
            TailnetClient::discover().with_operation_guard(_lifecycle.operation_guard())?;
        let device = tailnet.device()?;
        let origin = TailnetClient::origin(&device, https_port);
        let target = configured_endpoint(self.config.port);
        match tailnet.serve_port(&device.dns_name, https_port, &target)? {
            ServePort::Free => {}
            ServePort::Matching | ServePort::Occupied => bail!(
                "Tailscale Serve HTTPS port {https_port} is already configured and is not owned by TrueSpec"
            ),
        }
        let was_running = matches!(self.probe()?, Probe::Ready(_, _));
        self.down_local()?;
        let previous = self.config.clone();
        let enabled = TailnetConfig {
            enabled: true,
            https_port,
            origin: Some(origin),
            target: Some(target.clone()),
        };
        self.config = match self.config.save_tailnet(&self.paths, enabled) {
            Ok(config) => config,
            Err(error) => {
                if was_running && let Err(rollback) = self.up_local() {
                    return Err(error.context(format!(
                        "Cannot save tailnet ownership, and local-mode restart failed: {rollback:#}"
                    )));
                }
                return Err(error);
            }
        };
        let mut started = match self.up_local() {
            Ok(started) => started,
            Err(error) => {
                self.config = match previous.save_tailnet(&self.paths, previous.tailnet.clone()) {
                    Ok(config) => config,
                    Err(rollback) => {
                        return Err(error.context(format!(
                            "Cannot start TrueSpec with tailnet browser trust, and local configuration rollback failed: {rollback:#}"
                        )));
                    }
                };
                if was_running && let Err(rollback) = self.up_local() {
                    return Err(error.context(format!(
                        "Cannot start TrueSpec with tailnet browser trust, and local-mode restart failed: {rollback:#}"
                    )));
                }
                return Err(error.context("Cannot start TrueSpec with tailnet browser trust"));
            }
        };
        match tailnet.publish_if_free(&device.dns_name, https_port, &target) {
            Ok(PublishResult::Published) => {}
            Ok(PublishResult::BecameOccupied) => {
                self.rollback_tailnet_policy(&previous, was_running, &mut started)
                    .context("Cannot restore local mode after the Serve port changed ownership")?;
                bail!(
                    "Tailscale Serve HTTPS port {https_port} changed ownership during setup; previous local mode was restored"
                );
            }
            Err(error) => {
                let transition = TailnetTransition {
                    client: &tailnet,
                    dns_name: &device.dns_name,
                    https_port,
                    target: &target,
                };
                if let Err(rollback) =
                    self.rollback_tailnet_enable(&previous, was_running, &transition, &mut started)
                {
                    return Err(error.context(format!(
                        "Tailscale Serve setup failed, and rollback did not complete: {rollback:#}"
                    )));
                }
                return Err(
                    error.context("Tailscale Serve setup failed; previous local mode was restored")
                );
            }
        }
        if let Err(error) = self.reconcile_tailnet(&_lifecycle) {
            let transition = TailnetTransition {
                client: &tailnet,
                dns_name: &device.dns_name,
                https_port,
                target: &target,
            };
            if let Err(rollback) =
                self.rollback_tailnet_enable(&previous, was_running, &transition, &mut started)
            {
                return Err(error.context(format!(
                    "Tailnet browser readiness failed, and rollback did not complete: {rollback:#}"
                )));
            }
            return Err(
                error.context("Tailnet browser readiness failed; previous local mode was restored")
            );
        }
        self.status()
    }

    fn rollback_tailnet_enable(
        &mut self,
        previous: &ServiceConfig,
        was_running: bool,
        transition: &TailnetTransition<'_>,
        started: &mut Option<Child>,
    ) -> Result<()> {
        transition.client.disable(
            transition.dns_name,
            transition.https_port,
            transition.target,
        )?;
        self.rollback_tailnet_policy(previous, was_running, started)
    }

    fn rollback_tailnet_policy(
        &mut self,
        previous: &ServiceConfig,
        was_running: bool,
        started: &mut Option<Child>,
    ) -> Result<()> {
        self.down_local_owned(started.as_mut())?;
        self.config = previous.save_tailnet(&self.paths, previous.tailnet.clone())?;
        if was_running {
            drop(self.up_local()?);
        }
        Ok(())
    }

    fn up_local(&self) -> Result<Option<Child>> {
        match self.probe()? {
            Probe::Ready(_, _) => return Ok(None),
            Probe::Incompatible(_, _) => {
                bail!(
                    "A different TrueSpec version is running. Stop it with its matching binary before starting {}.",
                    env!("CARGO_PKG_VERSION")
                );
            }
            Probe::Stale(registration) => {
                registration.remove_if_current(&self.paths)?;
            }
            Probe::Failed(registration) if !process_alive(registration.pid) => {
                registration.remove_if_current(&self.paths)?;
            }
            Probe::Failed(_) => bail!(
                "The failed TrueSpec service owner has not exited yet. Wait briefly and retry."
            ),
            Probe::Starting(registration) => {
                return self.wait_for_ready(None, Some(registration));
            }
            Probe::Publishing => return self.wait_for_ready(None, None),
            Probe::TimedOut(_) | Probe::Unresponsive(_) => bail!(
                "The registered TrueSpec process is alive but unresponsive. Inspect the service status and stop it explicitly."
            ),
            Probe::EndpointCollision(_, endpoint) => bail!(
                "The configured TrueSpec endpoint {endpoint} is occupied by an unverified process. Stop that process or choose another port."
            ),
            Probe::Stopped => {}
        }
        ensure!(
            !MaintenanceLock::is_held(&self.paths)?,
            "TrueSpec maintenance is in progress; retry after it completes"
        );
        let child = self.spawn_contender()?;
        self.wait_for_ready(Some(child), None)
    }

    fn ready_registration(&self) -> Result<Registration> {
        match self.probe()? {
            Probe::Ready(registration, _) => Ok(registration),
            _ => bail!("TrueSpec service is not ready"),
        }
    }

    fn reconcile_tailnet(&self, lifecycle: &LifecycleLock) -> Result<()> {
        if !self.config.tailnet.enabled {
            return Ok(());
        }
        let origin = self
            .config
            .tailnet
            .origin
            .as_deref()
            .context("Tailnet mode has no HTTPS origin")?;
        let target = self
            .config
            .tailnet
            .target
            .as_deref()
            .context("Tailnet mode has no loopback target")?;
        let registration = self.ready_registration()?;
        ensure!(
            registration.url == target,
            "The running TrueSpec endpoint no longer matches its owned tailnet proxy target"
        );
        let client = TailnetClient::discover().with_operation_guard(lifecycle.operation_guard())?;
        let device = client.device()?;
        ensure!(
            TailnetClient::origin(&device, self.config.tailnet.https_port) == origin,
            "The current Tailscale DNS name no longer matches TrueSpec's owned tailnet origin"
        );
        client.enable(&device.dns_name, self.config.tailnet.https_port, target)?;
        client.verify_browser(origin)
    }

    fn tailnet_status(&self) -> TailnetStatus {
        let config = &self.config.tailnet;
        if !config.enabled {
            return TailnetStatus::disabled(config.https_port);
        }
        let verified = (|| -> Result<bool> {
            let origin = config
                .origin
                .as_deref()
                .context("Tailnet origin is missing")?;
            let target = config
                .target
                .as_deref()
                .context("Tailnet target is missing")?;
            let client = TailnetClient::discover();
            let device = client.device()?;
            Ok(TailnetClient::origin(&device, config.https_port) == origin
                && client.serve_port(&device.dns_name, config.https_port, target)?
                    == ServePort::Matching)
        })()
        .unwrap_or(false);
        TailnetStatus {
            enabled: true,
            https_port: config.https_port,
            origin: config.origin.clone(),
            verified,
        }
    }

    fn wait_for_ready(
        &self,
        contender: Option<Child>,
        expected: Option<Registration>,
    ) -> Result<Option<Child>> {
        let mut contender = contender.map(ContenderGuard::new);
        let deadline = Instant::now() + START_TIMEOUT;
        let mut retries = 0_u8;
        let mut retry_after = Instant::now() + Duration::from_millis(500);
        loop {
            let stopped = match self.probe()? {
                Probe::Ready(_, _) => {
                    return Ok(contender.as_mut().and_then(ContenderGuard::release));
                }
                Probe::Incompatible(_, _) => {
                    bail!("A newly discovered TrueSpec service is incompatible")
                }
                Probe::Failed(_) => {
                    bail!(
                        "The TrueSpec service failed during startup. Inspect its bounded service log."
                    )
                }
                Probe::Stale(registration) => {
                    registration.remove_if_current(&self.paths)?;
                    false
                }
                Probe::EndpointCollision(_, endpoint) => bail!(
                    "The configured TrueSpec endpoint {endpoint} is occupied by an unverified process."
                ),
                Probe::Stopped if expected.is_some() => {
                    bail!("The starting TrueSpec service disappeared before it became ready")
                }
                Probe::Stopped => true,
                _ => false,
            };
            if let Some(child) = contender.as_mut()
                && let Some(status) = child.try_wait()?
            {
                if !status.success() {
                    bail!("The TrueSpec service failed to start. Inspect its bounded service log.");
                }
                contender = None;
            }
            if stopped
                && expected.is_none()
                && contender.is_none()
                && retries < 2
                && Instant::now() >= retry_after
            {
                contender = Some(ContenderGuard::new(self.spawn_contender()?));
                retries += 1;
                retry_after = Instant::now() + Duration::from_millis(500);
            }
            ensure!(
                Instant::now() < deadline,
                "Timed out waiting for the TrueSpec service to start. Check {}.",
                self.paths.log().display()
            );
            thread::sleep(POLL_INTERVAL);
        }
    }

    pub fn down(&mut self) -> Result<bool> {
        let _lifecycle = LifecycleLock::acquire(&self.paths)?;
        self.config = ServiceConfig::load_or_create(&self.paths)?;
        let tailnet = (|| -> Result<Option<(TailnetClient, String, String)>> {
            if !self.config.tailnet.enabled {
                return Ok(None);
            }
            let origin = self
                .config
                .tailnet
                .origin
                .as_deref()
                .context("Tailnet mode has no HTTPS origin")?;
            let target = self
                .config
                .tailnet
                .target
                .as_deref()
                .context("Tailnet mode has no loopback target")?;
            let client =
                TailnetClient::discover().with_operation_guard(_lifecycle.operation_guard())?;
            let device = client.device()?;
            ensure!(
                TailnetClient::origin(&device, self.config.tailnet.https_port) == origin,
                "The current Tailscale DNS name no longer matches TrueSpec's owned tailnet origin"
            );
            ensure!(
                client.serve_port(&device.dns_name, self.config.tailnet.https_port, target)?
                    != ServePort::Occupied,
                "Tailscale Serve HTTPS port {} no longer matches TrueSpec",
                self.config.tailnet.https_port
            );
            Ok(Some((client, device.dns_name, target.to_owned())))
        })();
        let stopped = self.down_local()?;
        let tailnet = tailnet.with_context(|| {
            format!(
                "The local TrueSpec service was stopped, but Serve cleanup could not be verified. Inspect it before running `tailscale serve --https={} off`",
                self.config.tailnet.https_port
            )
        })?;
        if let Some((client, dns_name, target)) = tailnet {
            client.disable(&dns_name, self.config.tailnet.https_port, &target)?;
            self.config = self
                .config
                .save_tailnet(&self.paths, TailnetConfig::disabled())?;
        }
        Ok(stopped)
    }

    pub fn pause_for_maintenance(&mut self) -> Result<bool> {
        let _lifecycle = LifecycleLock::acquire(&self.paths)?;
        self.config = ServiceConfig::load_or_create(&self.paths)?;
        self.down_local()
    }

    fn down_local(&self) -> Result<bool> {
        self.down_local_owned(None)
    }

    fn down_local_owned(&self, mut owned: Option<&mut Child>) -> Result<bool> {
        let registration = match self.probe()? {
            Probe::Stopped => return Ok(false),
            Probe::Ready(registration, _) => registration,
            Probe::Stale(registration) | Probe::Failed(registration)
                if !process_alive(registration.pid) =>
            {
                registration.remove_if_current(&self.paths)?;
                return Ok(false);
            }
            Probe::Incompatible(_, _) => bail!(
                "The running TrueSpec service is incompatible. Stop it with its matching binary."
            ),
            Probe::EndpointCollision(_, _) => bail!(
                "The configured endpoint belongs to an unverified process; TrueSpec left it untouched."
            ),
            Probe::Starting(_)
            | Probe::Failed(_)
            | Probe::Stale(_)
            | Probe::TimedOut(_)
            | Probe::Unresponsive(_)
            | Probe::Publishing => bail!(
                "The registered TrueSpec process is not authenticated and ready; refusing to signal it."
            ),
        };
        let response = self
            .authorize(
                self.http
                    .post(format!("{}__control/shutdown", registration.url)),
            )
            .send()
            .context("Cannot request TrueSpec service shutdown")?;
        require_success(response)?;
        wait_for_exit(
            &registration,
            &self.paths,
            STOP_TIMEOUT,
            owned.as_deref_mut(),
        )?;
        if process_alive(registration.pid) {
            ensure_current_owner(&registration, &self.paths)?;
            ensure!(
                process_is_current_executable(registration.pid),
                "The registered PID no longer belongs to this TrueSpec executable; refusing to signal it"
            );
            signal(registration.pid, nix::sys::signal::Signal::SIGTERM)?;
            wait_for_exit(
                &registration,
                &self.paths,
                Duration::from_secs(3),
                owned.as_deref_mut(),
            )?;
        }
        if process_alive(registration.pid) {
            ensure_current_owner(&registration, &self.paths)?;
            ensure!(
                process_is_current_executable(registration.pid),
                "The registered PID changed ownership during shutdown; refusing to escalate"
            );
            signal(registration.pid, nix::sys::signal::Signal::SIGKILL)?;
            wait_for_exit(&registration, &self.paths, Duration::from_secs(2), owned)?;
        }
        ensure!(
            !process_alive(registration.pid),
            "TrueSpec service process {} did not stop",
            registration.pid
        );
        registration.remove_if_current(&self.paths)?;
        Ok(true)
    }

    pub fn register(&self, root: &Path) -> Result<WorkspaceRegistration> {
        let registration = match self.probe()? {
            Probe::Ready(registration, _) => registration,
            _ => bail!("TrueSpec service is not ready"),
        };
        let response = self
            .authorize(
                self.http
                    .post(format!("{}__control/workspaces/register", registration.url))
                    .json(&RegisterWorkspace {
                        root: root.to_path_buf(),
                    }),
            )
            .send()
            .context("Cannot register workspace with TrueSpec service")?;
        require_success(response)?
            .json()
            .context("TrueSpec service returned invalid workspace registration")
    }

    pub fn resolve(&self, root: &Path) -> Result<WorkspaceRegistration> {
        let registration = match self.probe()? {
            Probe::Ready(registration, _) => registration,
            _ => bail!("TrueSpec service is not ready"),
        };
        let response = self
            .authorize(
                self.http
                    .post(format!("{}__control/workspaces/resolve", registration.url))
                    .json(&RegisterWorkspace {
                        root: root.to_path_buf(),
                    }),
            )
            .send()
            .context("Cannot resolve workspace through TrueSpec service")?;
        require_success(response)?
            .json()
            .context("TrueSpec service returned invalid workspace resolution")
    }

    pub fn attach(&mut self, request: &AttachRequest) -> Result<AttachResponse> {
        let _lifecycle = LifecycleLock::acquire(&self.paths)?;
        self.config = ServiceConfig::load_or_create(&self.paths)?;
        self.reconcile_tailnet(&_lifecycle)?;
        self.post("__control/connectors/attach", request)?
            .json()
            .context("TrueSpec service returned invalid connector attachment")
    }

    pub fn poll(&self, request: &PollRequest) -> Result<serde_json::Value> {
        self.post_with_timeout(
            "__control/connectors/poll",
            request,
            Duration::from_secs(request.wait_seconds.saturating_add(2)),
        )?
        .json()
        .context("TrueSpec service returned invalid connector envelope")
    }

    pub fn delivered(&self, request: &DeliveryRequest) -> Result<serde_json::Value> {
        self.post("__control/connectors/delivered", request)?
            .json()
            .context("TrueSpec service returned invalid delivery result")
    }

    pub fn defer(&self, request: &DeferRequest) -> Result<serde_json::Value> {
        self.post("__control/connectors/defer", request)?
            .json()
            .context("TrueSpec service returned invalid defer result")
    }

    pub fn respond(&self, request: &RespondRequest) -> Result<serde_json::Value> {
        self.post("__control/connectors/respond", request)?
            .json()
            .context("TrueSpec service returned invalid response result")
    }

    fn post(&self, path: &str, body: &impl Serialize) -> Result<reqwest::blocking::Response> {
        self.post_with_timeout(path, body, REQUEST_TIMEOUT)
    }

    fn post_with_timeout(
        &self,
        path: &str,
        body: &impl Serialize,
        timeout: Duration,
    ) -> Result<reqwest::blocking::Response> {
        let registration = match self.probe()? {
            Probe::Ready(registration, _) => registration,
            _ => bail!("TrueSpec service is not ready"),
        };
        let response = self
            .authorize(
                self.http
                    .post(format!("{}{path}", registration.url))
                    .timeout(timeout)
                    .json(body),
            )
            .send()
            .context("Cannot reach TrueSpec service")?;
        require_success(response)
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        request.bearer_auth(&self.config.token)
    }

    fn spawn_contender(&self) -> Result<Child> {
        let executable = std::env::current_exe()?.canonicalize()?;
        let mut command = Command::new(executable);
        command
            .args(["service", "run"])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        copy_environment(&mut command, "HOME");
        copy_environment(&mut command, "TMPDIR");
        command
            .env(
                "XDG_CONFIG_HOME",
                self.paths
                    .config_dir
                    .parent()
                    .context("Invalid service config directory")?,
            )
            .env(
                "XDG_STATE_HOME",
                self.paths
                    .state_dir
                    .parent()
                    .context("Invalid service state directory")?,
            )
            .env(
                "XDG_DATA_HOME",
                self.paths
                    .data_dir
                    .parent()
                    .context("Invalid service data directory")?,
            );
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        command
            .spawn()
            .context("Cannot start TrueSpec background service")
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct RegisterWorkspace {
    root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkspaceRegistration {
    pub workspace_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AttachRequest {
    pub protocol: u8,
    pub root: PathBuf,
    pub kind: String,
    pub destination: String,
    pub label: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AttachResponse {
    pub protocol_version: u8,
    pub workspace_id: String,
    pub workspace: PathBuf,
    pub connection_id: String,
    pub review_id: String,
    pub url: String,
    pub local_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailnet_origin: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PollRequest {
    pub protocol: u8,
    pub workspace_id: String,
    pub connection: String,
    pub wait_seconds: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeliveryRequest {
    pub protocol: u8,
    pub workspace_id: String,
    pub connection: String,
    pub batch_id: String,
    pub lease: String,
    pub receipt: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeferRequest {
    pub protocol: u8,
    pub workspace_id: String,
    pub connection: String,
    pub batch_id: String,
    pub lease: String,
    pub delay_seconds: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RespondRequest {
    pub protocol: u8,
    pub workspace_id: String,
    pub connection: String,
    pub batch_id: String,
    pub reply_id: String,
    pub body: String,
}

pub fn authorized(header: Option<&str>, expected: &ServiceConfig) -> bool {
    let Some(token) = header.and_then(|value| value.strip_prefix("Bearer ")) else {
        return false;
    };
    token.len() == expected.token.len()
        && bool::from(token.as_bytes().ct_eq(expected.token.as_bytes()))
}

fn registration_lock(paths: &ServicePaths) -> Result<File> {
    let file = secure_open(&paths.registration_lock(), true, false)?;
    FileExt::lock_exclusive(&file).context("Cannot lock TrueSpec service registration")?;
    Ok(file)
}

fn append_log(paths: &ServicePaths, event: &serde_json::Value) -> Result<()> {
    paths.ensure()?;
    let lock = secure_open(&paths.log_lock(), true, false)?;
    FileExt::lock_exclusive(&lock).context("Cannot lock the TrueSpec service log")?;
    let mut bytes = serde_json::to_vec(event)?;
    bytes.push(b'\n');
    ensure!(
        u64::try_from(bytes.len())? <= LOG_LIMIT,
        "TrueSpec service log event is too large"
    );
    let path = paths.log();
    if fs::metadata(&path).is_ok_and(|metadata| {
        metadata
            .len()
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
            > LOG_LIMIT
    }) {
        let previous = path.with_extension("log.1");
        let _ = fs::remove_file(&previous);
        fs::rename(&path, previous).context("Cannot rotate TrueSpec service log")?;
    }
    let mut file = secure_open(&path, true, true)?;
    file.write_all(&bytes)?;
    file.sync_data()?;
    Ok(())
}

fn run_daemon() -> Result<()> {
    let paths = ServicePaths::resolve().context(StartupPhase::Paths)?;
    let Some(owner) = OwnerLock::acquire(&paths).context(StartupPhase::Ownership)? else {
        return Ok(());
    };
    if MaintenanceLock::is_held(&paths).context(StartupPhase::Ownership)? {
        return Ok(());
    }
    let config = ServiceConfig::load_or_create(&paths).context(StartupPhase::Configuration)?;
    let registry = ProjectRegistry::open(&paths).context(StartupPhase::Registry)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context(StartupPhase::Runtime)?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, config.port()))
            .await
            .context(StartupPhase::EndpointBind)?;
        let address = listener.local_addr().context(StartupPhase::EndpointBind)?;
        let mut registration = Registration {
            protocol: SERVICE_PROTOCOL,
            version: env!("CARGO_PKG_VERSION").into(),
            instance_id: Uuid::new_v4().to_string(),
            generation: Uuid::new_v4().to_string(),
            pid: std::process::id(),
            url: format!("http://{address}/"),
            state: RegistrationState::Starting,
        };
        publish_registration(&paths, &mut registration)?;
        let result = crate::web::serve_shared(
            listener,
            registry,
            config,
            registration.clone(),
            paths.clone(),
            owner,
        )
        .await
        .context(StartupPhase::Serving);
        match result {
            Ok(()) => registration.remove_if_current(&paths),
            Err(error) => {
                registration.state = RegistrationState::Failed;
                registration
                    .write(&paths)
                    .context(StartupPhase::Registration)?;
                Err(error)
            }
        }
    })
}

fn publish_registration(paths: &ServicePaths, registration: &mut Registration) -> Result<()> {
    let instance_id = registration.instance_id.clone();
    let pid = registration.pid;
    publish_registration_with(paths, registration, move || {
        append_log(
            paths,
            &serde_json::json!({
                "level": "INFO",
                "event": "service_starting",
                "instance_id": instance_id,
                "pid": pid
            }),
        )
        .context(StartupPhase::Logging)
    })
}

fn publish_registration_with(
    paths: &ServicePaths,
    registration: &mut Registration,
    log: impl FnOnce() -> Result<()>,
) -> Result<()> {
    registration.state = RegistrationState::Starting;
    registration
        .write(paths)
        .context(StartupPhase::Registration)?;
    log()
}

pub fn run_daemon_entry() -> Result<()> {
    match run_daemon() {
        Ok(()) => return Ok(()),
        Err(error) => {
            let phase = startup_failure_phase(&error);
            if let Ok(paths) = ServicePaths::resolve() {
                let _ = append_log(
                    &paths,
                    &serde_json::json!({
                        "level": "ERROR",
                        "event": "service_failed",
                        "phase": phase,
                        "recovery": "Run `truespec status`; inspect the named startup phase without exposing private values."
                    }),
                );
            }
        }
    }
    bail!("TrueSpec background service failed")
}

pub fn reset_stopped_workspace(
    root: &Path,
    _maintenance: &MaintenanceLock,
) -> Result<ResetOutcome> {
    let paths = ServicePaths::resolve()?;
    let owner = OwnerLock::acquire(&paths)?
        .context("TrueSpec service is still running; stop it before resetting state")?;
    let registry = ProjectRegistry::open(&paths)?;
    let mut state = crate::review::quarantine_state(root)?;
    let forgotten = match registry.forget(root, &owner) {
        Ok(forgotten) => forgotten.is_some(),
        Err(error) => {
            state
                .rollback()
                .context("Cannot restore state after route revocation failed")?;
            return Err(error);
        }
    };
    if let Err(error) = registry.register(root) {
        state
            .rollback()
            .context("Cannot restore state after fresh registration failed")?;
        if forgotten {
            registry
                .register(root)
                .context("Cannot restore the original route after fresh registration failed")?;
        }
        return Err(error);
    }
    let (reset, cleanup_incomplete) = state.commit();
    drop(owner);
    Ok(ResetOutcome {
        reset,
        cleanup_incomplete,
    })
}

fn startup_failure_phase(error: &anyhow::Error) -> &'static str {
    error
        .downcast_ref::<StartupPhase>()
        .map(StartupPhase::label)
        .unwrap_or("unknown")
}

fn status_from_registration(
    state: &'static str,
    registration: Registration,
    detail: &str,
) -> Status {
    Status {
        state,
        version: Some(registration.version),
        pid: Some(registration.pid),
        endpoint: Some(registration.url),
        projects: None,
        open_reviews: None,
        tailnet: TailnetStatus::disabled(DEFAULT_TAILNET_PORT),
        detail: Some(detail.into()),
    }
}

fn configured_endpoint(port: u16) -> String {
    format!("http://{}:{port}/", Ipv4Addr::LOCALHOST)
}

fn endpoint_occupied(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_millis(200),
    )
    .is_ok()
}

fn copy_environment(command: &mut Command, name: &str) {
    if let Some(value) = std::env::var_os(name) {
        command.env(name, value);
    }
}

fn wait_for_exit(
    registration: &Registration,
    paths: &ServicePaths,
    timeout: Duration,
    mut owned: Option<&mut Child>,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(child) = owned.as_mut()
            && child.try_wait()?.is_some()
        {
            return Ok(());
        }
        if !process_alive(registration.pid) {
            return Ok(());
        }
        ensure_not_replaced(registration, paths)?;
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

fn ensure_not_replaced(registration: &Registration, paths: &ServicePaths) -> Result<()> {
    let current = Registration::read(paths)?;
    ensure!(
        current.is_none() || current.as_ref() == Some(registration),
        "The TrueSpec service registration changed during the lifecycle operation"
    );
    Ok(())
}

fn ensure_current_owner(registration: &Registration, paths: &ServicePaths) -> Result<()> {
    ensure!(
        Registration::read(paths)?.as_ref() == Some(registration),
        "The TrueSpec service registration changed during the lifecycle operation"
    );
    Ok(())
}

fn process_is_current_executable(pid: u32) -> bool {
    let Ok(current) = std::env::current_exe().and_then(|path| path.canonicalize()) else {
        return false;
    };
    #[cfg(target_os = "linux")]
    {
        fs::read_link(format!("/proc/{pid}/exe"))
            .and_then(|path| path.canonicalize())
            .is_ok_and(|path| path == current)
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output();
        output
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| PathBuf::from(value.trim()))
            .and_then(|path| path.canonicalize().ok())
            .is_some_and(|path| path == current)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        false
    }
}

fn process_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

fn signal(pid: u32, signal: nix::sys::signal::Signal) -> Result<()> {
    let pid = i32::try_from(pid).context("Service PID is outside the supported range")?;
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), signal)
        .context("Cannot signal TrueSpec service")
}

fn secure_directory(path: &Path) -> Result<()> {
    ensure_no_symlink_components(path)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .with_context(|| format!("Cannot create private directory {}", path.display()))?;
    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Private service path must be a real directory"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn ensure_no_symlink_components(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Private service paths must be absolute");
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "Private service paths must not contain symlinks"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error).context("Cannot validate private service path"),
        }
    }
    Ok(())
}

fn secure_open(path: &Path, create: bool, append: bool) -> Result<File> {
    if let Some(parent) = path.parent() {
        secure_directory(parent)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(create).append(append);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .with_context(|| format!("Cannot open private service file {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "Private service path must be a regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "Private service record must be a regular file"
    );
    ensure!(
        metadata.len() <= FILE_LIMIT,
        "Private service record is too large"
    );
    let mut bytes = Vec::new();
    secure_open(path, false, false)?
        .take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= FILE_LIMIT,
        "Private service record is too large"
    );
    serde_json::from_slice(&bytes).context("Private service record is invalid")
}

fn create_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if let Some(parent) = path.parent() {
        secure_directory(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    match fs::hard_link(&temporary, path) {
        Ok(()) => {
            fs::remove_file(&temporary)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(&temporary)?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error).context("Cannot publish TrueSpec service configuration")
        }
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if let Some(parent) = path.parent() {
        secure_directory(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&temporary, path).context("Cannot publish TrueSpec service record")
}

pub fn require_success(
    response: reqwest::blocking::Response,
) -> Result<reqwest::blocking::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let text = response.text().unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        bail!("TrueSpec service rejected its private control credential");
    }
    bail!(
        "TrueSpec service request failed with {status}: {}",
        text.trim()
    )
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::TcpListener,
        thread,
    };

    use super::*;

    fn paths(root: &Path) -> ServicePaths {
        let root = root.canonicalize().unwrap();
        ServicePaths {
            config_dir: root.join("config/truespec"),
            state_dir: root.join("state/truespec"),
            data_dir: root.join("data/truespec"),
        }
    }

    fn config(paths: &ServicePaths, port: u16) -> ServiceConfig {
        paths.ensure().unwrap();
        let config = ServiceConfig {
            version: CONFIG_VERSION,
            port,
            token: "a".repeat(64),
            tailnet: TailnetConfig::disabled(),
        };
        write_json(&paths.config(), &config).unwrap();
        config
    }

    fn registration(url: String, state: RegistrationState) -> Registration {
        Registration {
            protocol: SERVICE_PROTOCOL,
            version: env!("CARGO_PKG_VERSION").into(),
            instance_id: Uuid::new_v4().to_string(),
            generation: Uuid::new_v4().to_string(),
            pid: std::process::id(),
            url,
            state,
        }
    }

    fn response_server(body: String, requests: usize) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for stream in listener.incoming().take(requests) {
                let mut stream = stream.unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        });
        (url, handle)
    }

    #[test]
    fn creates_private_config_and_keeps_domains_separate() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let paths = paths(fixture.path());
        let config = ServiceConfig::load_or_create(&paths).unwrap();
        assert_eq!(config.port(), DEFAULT_PORT);
        assert_eq!(config.version, 2);
        assert_eq!(config.tailnet, TailnetConfig::disabled());
        assert_eq!(
            ServiceConfig::load_or_create(&paths).unwrap().token,
            config.token
        );
        assert_eq!(
            paths.registration(),
            root.join("state/truespec/service.json")
        );
        assert_eq!(
            paths.registry(),
            root.join("data/truespec/registry.sqlite3")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(paths.config()).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&paths.state_dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn loads_v1_config_without_stranding_a_running_legacy_owner() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        paths.ensure().unwrap();
        write_json(
            &paths.config(),
            &serde_json::json!({
                "version": 1,
                "port": 47371,
                "token": "a".repeat(64)
            }),
        )
        .unwrap();
        let config = ServiceConfig::load_or_create(&paths).unwrap();
        assert_eq!(config.version, 2);
        assert_eq!(config.port, 47371);
        assert_eq!(config.token, "a".repeat(64));
        assert_eq!(config.tailnet, TailnetConfig::disabled());
        let serialized = fs::read_to_string(paths.config()).unwrap();
        assert!(serialized.contains("\"version\": 1"));
        assert!(!serialized.contains("\"tailnet\""));
        assert!(!serialized.contains("Tailscale-User"));
        assert!(!serialized.contains("reviewer@"));
    }

    #[test]
    fn resolves_complete_absolute_xdg_paths_without_home() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let paths = ServicePaths::from_bases(
            None,
            Some(root.join("config")),
            Some(root.join("state")),
            Some(root.join("data")),
        )
        .unwrap();
        assert_eq!(paths.config_dir, root.join("config/truespec"));
        assert_eq!(paths.state_dir, root.join("state/truespec"));
        assert_eq!(paths.data_dir, root.join("data/truespec"));
    }

    #[test]
    fn registration_replacement_is_identity_checked() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        paths.ensure().unwrap();
        let first = Registration {
            protocol: SERVICE_PROTOCOL,
            version: "1".into(),
            instance_id: Uuid::new_v4().to_string(),
            generation: Uuid::new_v4().to_string(),
            pid: 10,
            url: "http://127.0.0.1:47371/".into(),
            state: RegistrationState::Ready,
        };
        let second = Registration {
            generation: Uuid::new_v4().to_string(),
            ..first.clone()
        };
        first.write(&paths).unwrap();
        second.write(&paths).unwrap();
        first.remove_if_current(&paths).unwrap();
        assert_eq!(Registration::read(&paths).unwrap(), Some(second.clone()));
        second.remove_if_current(&paths).unwrap();
        assert!(Registration::read(&paths).unwrap().is_none());
    }

    #[test]
    fn concurrent_registration_replacement_survives_stale_removal() {
        for _ in 0..100 {
            let fixture = tempfile::tempdir().unwrap();
            let paths = paths(fixture.path());
            paths.ensure().unwrap();
            let first = Registration {
                protocol: SERVICE_PROTOCOL,
                version: "1".into(),
                instance_id: Uuid::new_v4().to_string(),
                generation: Uuid::new_v4().to_string(),
                pid: 10,
                url: "http://127.0.0.1:47371/".into(),
                state: RegistrationState::Ready,
            };
            let replacement = Registration {
                instance_id: Uuid::new_v4().to_string(),
                generation: Uuid::new_v4().to_string(),
                ..first.clone()
            };
            first.write(&paths).unwrap();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
            let remove = {
                let barrier = barrier.clone();
                let paths = paths.clone();
                let first = first.clone();
                thread::spawn(move || {
                    barrier.wait();
                    first.remove_if_current(&paths).unwrap();
                })
            };
            let replace = {
                let barrier = barrier.clone();
                let paths = paths.clone();
                let replacement = replacement.clone();
                thread::spawn(move || {
                    barrier.wait();
                    replacement.write(&paths).unwrap();
                })
            };
            barrier.wait();
            remove.join().unwrap();
            replace.join().unwrap();
            assert_eq!(Registration::read(&paths).unwrap(), Some(replacement));
        }
    }

    #[test]
    fn rejects_malformed_and_oversized_private_records() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        paths.ensure().unwrap();
        fs::write(
            paths.config(),
            b"{\"version\":1,\"port\":1,\"token\":\"leak\"}",
        )
        .unwrap();
        assert!(ServiceConfig::load_or_create(&paths).is_err());
        fs::remove_file(paths.config()).unwrap();
        fs::write(paths.registration(), b"not json").unwrap();
        assert!(Registration::read(&paths).is_err());
        fs::write(paths.registration(), vec![b'x'; FILE_LIMIT as usize + 1]).unwrap();
        assert!(Registration::read(&paths).is_err());
    }

    #[test]
    fn startup_failure_logging_keeps_only_a_safe_phase() {
        let error = anyhow::anyhow!("private/path and credential").context(StartupPhase::Registry);
        assert_eq!(startup_failure_phase(&error), "registry");
        let event = serde_json::json!({
            "event": "service_failed",
            "phase": startup_failure_phase(&error)
        })
        .to_string();
        assert!(!event.contains("private/path"));
        assert!(!event.contains("credential"));
    }

    #[test]
    fn failure_before_serving_never_publishes_ready_registration() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        let mut registration = registration(
            "http://127.0.0.1:47371/".into(),
            RegistrationState::Starting,
        );
        assert!(
            publish_registration_with(&paths, &mut registration, || {
                anyhow::bail!("simulated bounded log failure")
            })
            .is_err()
        );
        assert_eq!(
            Registration::read(&paths).unwrap().unwrap().state,
            RegistrationState::Starting
        );
    }

    #[test]
    fn pre_serving_publication_remains_starting_until_authenticated_health() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        let mut registration = registration(
            "http://127.0.0.1:47371/".into(),
            RegistrationState::Starting,
        );
        publish_registration_with(&paths, &mut registration, || Ok(())).unwrap();
        assert_eq!(registration.state, RegistrationState::Starting);
        assert_eq!(
            Registration::read(&paths).unwrap().unwrap().state,
            RegistrationState::Starting
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_intermediate_global_directories() {
        use std::os::unix::fs::symlink;

        let fixture = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        symlink(outside.path(), root.join("redirect")).unwrap();
        let paths = ServicePaths {
            config_dir: root.join("redirect/truespec"),
            state_dir: root.join("state/truespec"),
            data_dir: root.join("data/truespec"),
        };
        assert!(ServiceConfig::load_or_create(&paths).is_err());
        assert!(!outside.path().join("truespec").exists());
    }

    #[test]
    fn compares_bearer_credentials_without_prefix_fallbacks() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        let config = ServiceConfig::load_or_create(&paths).unwrap();
        assert!(authorized(
            Some(&format!("Bearer {}", config.token)),
            &config
        ));
        assert!(!authorized(
            Some(&format!("Bearer {}x", config.token)),
            &config
        ));
        assert!(!authorized(Some(&config.token), &config));
    }

    #[test]
    fn machine_status_reports_unknown_review_count_explicitly() {
        let status = Status {
            state: "running",
            version: Some("test".into()),
            pid: Some(1),
            endpoint: Some("http://127.0.0.1:1/".into()),
            projects: Some(1),
            open_reviews: None,
            tailnet: TailnetStatus::disabled(DEFAULT_TAILNET_PORT),
            detail: None,
        };
        let value = serde_json::to_value(status).unwrap();
        assert!(value.get("open_reviews").is_some());
        assert!(value["open_reviews"].is_null());
    }

    #[test]
    fn maintenance_lock_blocks_service_start_ownership() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        paths.ensure().unwrap();
        let file = secure_open(&paths.maintenance_lock(), true, false).unwrap();
        FileExt::try_lock_exclusive(&file).unwrap();
        assert!(MaintenanceLock::is_held(&paths).unwrap());
        FileExt::unlock(&file).unwrap();
        assert!(!MaintenanceLock::is_held(&paths).unwrap());
    }

    #[test]
    fn recognizes_the_current_executable_before_process_escalation() {
        assert!(process_is_current_executable(std::process::id()));
    }

    #[cfg(unix)]
    #[test]
    fn unverified_contender_is_killed_and_reaped_on_error() {
        let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        drop(ContenderGuard::new(child));
        assert!(!process_alive(pid));
    }

    #[test]
    fn classifies_starting_failed_stale_and_endpoint_collision_states() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        config(&paths, 0);
        let client = ServiceClient::with_paths(paths.clone()).unwrap();

        let starting = registration("http://127.0.0.1:9/".into(), RegistrationState::Starting);
        starting.write(&paths).unwrap();
        assert!(matches!(client.probe().unwrap(), Probe::Starting(_)));
        assert_eq!(client.status().unwrap().state, "starting");

        let failed = Registration {
            state: RegistrationState::Failed,
            ..starting
        };
        failed.write(&paths).unwrap();
        assert!(matches!(client.probe().unwrap(), Probe::Failed(_)));
        assert_eq!(client.status().unwrap().state, "failed");

        let stale = Registration {
            pid: u32::MAX,
            state: RegistrationState::Ready,
            ..failed
        };
        stale.write(&paths).unwrap();
        assert!(matches!(client.probe().unwrap(), Probe::Stale(_)));
        assert_eq!(client.status().unwrap().state, "stale");

        stale.remove_if_current(&paths).unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        config(&paths, port);
        let client = ServiceClient::with_paths(paths).unwrap();
        assert!(matches!(
            client.probe().unwrap(),
            Probe::EndpointCollision(None, _)
        ));
        assert_eq!(client.status().unwrap().state, "endpoint_collision");
        drop(listener);
    }

    #[test]
    fn classifies_ready_incompatible_and_unrelated_health_endpoints() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        config(&paths, 0);

        let template = registration(String::new(), RegistrationState::Ready);
        let health = Health {
            protocol: template.protocol,
            version: template.version.clone(),
            instance_id: template.instance_id.clone(),
            pid: template.pid,
            state: "ready".into(),
            projects: 2,
            open_reviews: Some(1),
        };
        let (url, server) = response_server(serde_json::to_string(&health).unwrap(), 1);
        let ready = Registration {
            url,
            ..template.clone()
        };
        ready.write(&paths).unwrap();
        let client = ServiceClient::with_paths(paths.clone()).unwrap();
        assert!(matches!(client.probe().unwrap(), Probe::Ready(_, _)));
        server.join().unwrap();

        let old_health = Health {
            protocol: SERVICE_PROTOCOL + 1,
            version: "0.0.0-test".into(),
            instance_id: template.instance_id.clone(),
            pid: template.pid,
            state: "ready".into(),
            projects: 0,
            open_reviews: Some(0),
        };
        let (url, server) = response_server(serde_json::to_string(&old_health).unwrap(), 1);
        Registration {
            protocol: old_health.protocol,
            version: old_health.version.clone(),
            url,
            ..template.clone()
        }
        .write(&paths)
        .unwrap();
        assert!(matches!(client.probe().unwrap(), Probe::Incompatible(_, _)));
        server.join().unwrap();

        let (url, server) = response_server("{}".into(), 1);
        Registration { url, ..template }.write(&paths).unwrap();
        assert!(matches!(
            client.probe().unwrap(),
            Probe::EndpointCollision(Some(_), _)
        ));
        server.join().unwrap();
    }

    #[test]
    fn classifies_a_bounded_health_timeout() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        config(&paths, 0);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let registration = registration(
            format!("http://{}/", listener.local_addr().unwrap()),
            RegistrationState::Ready,
        );
        registration.write(&paths).unwrap();
        let server = thread::spawn(move || {
            let _stream = listener.accept().unwrap().0;
            thread::sleep(REQUEST_TIMEOUT + Duration::from_secs(1));
        });
        let client = ServiceClient::with_paths(paths).unwrap();
        assert!(matches!(client.probe().unwrap(), Probe::TimedOut(_)));
        server.join().unwrap();
    }

    #[test]
    fn shutdown_refuses_to_signal_after_registration_replacement() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        config(&paths, 0);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let original = registration(
            format!("http://{}/", listener.local_addr().unwrap()),
            RegistrationState::Ready,
        );
        let replacement = Registration {
            instance_id: Uuid::new_v4().to_string(),
            generation: Uuid::new_v4().to_string(),
            ..original.clone()
        };
        original.write(&paths).unwrap();
        let health = Health {
            protocol: original.protocol,
            version: original.version.clone(),
            instance_id: original.instance_id.clone(),
            pid: original.pid,
            state: "ready".into(),
            projects: 0,
            open_reviews: Some(0),
        };
        let server_paths = paths.clone();
        let expected_replacement = replacement.clone();
        let server = thread::spawn(move || {
            for request_number in 0..2 {
                let mut stream = listener.accept().unwrap().0;
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                if request_number == 0 {
                    let body = serde_json::to_string(&health).unwrap();
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                } else {
                    expected_replacement.write(&server_paths).unwrap();
                    write!(
                        stream,
                        "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                }
            }
        });
        let mut client = ServiceClient::with_paths(paths.clone()).unwrap();
        let error = client.down().unwrap_err().to_string();
        assert!(error.contains("registration changed"));
        assert_eq!(Registration::read(&paths).unwrap(), Some(replacement));
        server.join().unwrap();
    }

    #[test]
    fn shutdown_escalation_requires_registration_to_remain_present() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        let registration = registration("http://127.0.0.1:9/".into(), RegistrationState::Ready);
        registration.write(&paths).unwrap();
        registration.remove_if_current(&paths).unwrap();
        assert!(ensure_not_replaced(&registration, &paths).is_ok());
        assert!(ensure_current_owner(&registration, &paths).is_err());
    }

    #[test]
    fn private_files_and_rotated_logs_remain_owner_only_and_bounded() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = paths(fixture.path());
        ServiceConfig::load_or_create(&paths).unwrap();
        append_log(&paths, &serde_json::json!({"event": "initial"})).unwrap();
        fs::write(paths.log(), vec![b'x'; LOG_LIMIT as usize]).unwrap();
        append_log(&paths, &serde_json::json!({"event": "rotated"})).unwrap();
        assert!(fs::metadata(paths.log()).unwrap().len() < LOG_LIMIT);
        assert_eq!(
            fs::metadata(paths.log().with_extension("log.1"))
                .unwrap()
                .len(),
            LOG_LIMIT
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for directory in [&paths.config_dir, &paths.state_dir, &paths.data_dir] {
                assert_eq!(
                    fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                    0o700
                );
            }
            assert_eq!(
                fs::metadata(paths.log()).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
