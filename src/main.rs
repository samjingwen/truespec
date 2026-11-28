mod anchor;
mod inbox;
mod initialize;
mod integration;
mod markdown;
mod project;
mod review;
mod service;
mod service_registry;
mod source;
mod web;

use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use inbox::PROTOCOL_VERSION;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize this repository and install coding integrations.
    Init {
        /// Document source adapter. Detected when omitted.
        #[arg(long)]
        source: Option<String>,
        /// Source directory relative to the repository root.
        #[arg(long)]
        source_path: Option<PathBuf>,
        /// Coding integration to install. Repeat to install several.
        #[arg(long = "integration", conflicts_with = "all")]
        integration: Vec<String>,
        /// Install every supported integration.
        #[arg(long, conflicts_with = "integration")]
        all: bool,
        /// Replace differing generated integration files.
        #[arg(long)]
        force: bool,
        /// Permanently discard local review state and create a new workspace identity.
        #[arg(long)]
        reset_state: bool,
    },
    /// Start the shared TrueSpec background service.
    Up {
        /// Emit machine-readable status.
        #[arg(long)]
        json: bool,
    },
    /// Show shared TrueSpec background service status.
    Status {
        /// Emit machine-readable status.
        #[arg(long)]
        json: bool,
    },
    /// Stop the shared TrueSpec background service.
    Down {
        /// Emit machine-readable status.
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    #[command(hide = true)]
    Connector {
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, default_value_t = PROTOCOL_VERSION)]
        protocol: u8,
        #[command(subcommand)]
        command: ConnectorCommand,
    },
}

#[derive(Subcommand)]
enum ServiceCommand {
    Run,
}

#[derive(Subcommand)]
enum ConnectorCommand {
    /// Pair with the open review or start a new review if the previous one ended.
    Attach {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        destination: String,
        #[arg(long)]
        label: String,
    },
    /// Lease the oldest undelivered batch for this connection.
    Poll {
        #[arg(long)]
        connection: String,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(0..=300))]
        wait: u64,
    },
    /// Record that the paired conversation accepted a batch.
    Delivered {
        batch_id: String,
        #[arg(long)]
        connection: String,
        #[arg(long)]
        lease: String,
        #[arg(long)]
        receipt: String,
    },
    /// Release a lease and retry delivery after a delay.
    Defer {
        batch_id: String,
        #[arg(long)]
        connection: String,
        #[arg(long)]
        lease: String,
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=300))]
        retry_after: u32,
    },
    /// Store one completed harness reply for a delivered batch.
    Respond {
        batch_id: String,
        #[arg(long)]
        connection: String,
        #[arg(long)]
        reply_id: String,
        /// Read Markdown from this file, or use - for stdin.
        #[arg(long)]
        body_file: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Init {
            source,
            source_path,
            integration,
            all,
            force,
            reset_state,
        } => {
            let root = project::ProjectConfig::repository_root()?;
            let config = project::ProjectConfig::initialize(
                &root,
                source.as_deref(),
                source_path.as_deref(),
            )?;
            let integrations = initialize::select_integrations(&integration, all)?;
            let context = integration::InstallContext {
                binary: std::env::current_exe()?.canonicalize()?,
                config_dir: integration::config_dir()?,
            };
            let plan = initialize::InitPlan::build(&root, &config, &integrations, &context, force)?;
            let client = service::ServiceClient::open()?;
            client.up()?;
            let changed = plan.apply()?;
            let state_reset = if reset_state {
                eprintln!(
                    "Resetting local TrueSpec review state. Drafts, reviews, batches, replies, and connector bindings will be permanently deleted."
                );
                let maintenance = service::MaintenanceLock::acquire()?;
                client.down()?;
                let reset = service::reset_stopped_workspace(&root, &maintenance);
                drop(maintenance);
                let reset = match reset {
                    Ok(reset) => reset,
                    Err(error) => {
                        let _ = client.up();
                        return Err(error);
                    }
                };
                if reset.cleanup_incomplete {
                    eprintln!(
                        "Fresh review state is active, but quarantined old state could not be fully deleted. Inspect `truespec/` for a `.state-reset-*` directory."
                    );
                }
                client.up()?;
                reset.reset
            } else {
                false
            };
            let registered = client.register(&root).context(
                "TrueSpec initialized the repository files, but workspace registration did not complete. Re-run `truespec init` after fixing the service error.",
            )?;
            println!("Initialized TrueSpec in {}", root.display());
            println!(
                "Document source: {} ({})",
                config.adapter().label(),
                config.source_path().display()
            );
            for path in changed {
                println!("Installed {}", path.display());
            }
            if state_reset {
                println!("Reset local review state.");
            }
            println!("Workspace: {}", registered.workspace_id);
            Ok(())
        }
        Command::Up { json } => {
            let status = service::ServiceClient::open()?.up()?;
            print_service_status(&status, json)
        }
        Command::Status { json } => {
            let status = service::ServiceClient::open()?.status()?;
            print_service_status(&status, json)
        }
        Command::Down { json } => {
            let client = service::ServiceClient::open()?;
            let stopped = client.down()?;
            let status = client.status()?;
            if !json {
                println!(
                    "{}",
                    if stopped {
                        "TrueSpec service stopped."
                    } else {
                        "TrueSpec service is already stopped."
                    }
                );
            }
            print_service_status(&status, json)
        }
        Command::Service {
            command: ServiceCommand::Run,
        } => service::run_daemon_entry(),
        Command::Connector {
            root,
            protocol,
            command,
        } => {
            ensure!(
                protocol == PROTOCOL_VERSION,
                "Unsupported connector protocol {protocol}; expected {PROTOCOL_VERSION}."
            );
            connector(root.as_deref(), command)
        }
    }
}

fn print_service_status(status: &service::Status, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(status)?);
        return Ok(());
    }
    println!("TrueSpec service: {}", status.state);
    if let Some(version) = &status.version {
        println!("Version: {version}");
    }
    if let Some(pid) = status.pid {
        println!("PID: {pid}");
    }
    if let Some(endpoint) = &status.endpoint {
        println!("Endpoint: {endpoint}");
    }
    if let Some(projects) = status.projects {
        println!("Projects: {projects}");
    }
    if let Some(open_reviews) = status.open_reviews {
        println!("Open reviews: {open_reviews}");
    } else if matches!(status.state, "running" | "incompatible") {
        println!("Open reviews: unknown (workspace state is still loading)");
    }
    if let Some(detail) = &status.detail {
        println!("Detail: {detail}");
    }
    Ok(())
}

fn connector(root: Option<&std::path::Path>, command: ConnectorCommand) -> Result<()> {
    let root = configured_root(root)?;
    if matches!(&command, ConnectorCommand::Attach { .. }) {
        let config = project::ProjectConfig::load(&root)?;
        config.adapter().open(&root, config.source_path())?;
    }
    let client = service::ServiceClient::open()?;
    client.up()?;
    let output = match command {
        ConnectorCommand::Attach {
            kind: name,
            destination,
            label,
        } => serde_json::to_value(client.attach(&service::AttachRequest {
            protocol: PROTOCOL_VERSION,
            root,
            kind: name,
            destination,
            label,
        })?)?,
        ConnectorCommand::Poll { connection, wait } => {
            let workspace = client.resolve(&root)?;
            client.poll(&service::PollRequest {
                protocol: PROTOCOL_VERSION,
                workspace_id: workspace.workspace_id,
                connection,
                wait_seconds: wait,
            })?
        }
        ConnectorCommand::Delivered {
            batch_id,
            connection,
            lease,
            receipt,
        } => {
            let workspace = client.resolve(&root)?;
            client.delivered(&service::DeliveryRequest {
                protocol: PROTOCOL_VERSION,
                workspace_id: workspace.workspace_id,
                connection,
                batch_id,
                lease,
                receipt,
            })?
        }
        ConnectorCommand::Defer {
            batch_id,
            connection,
            lease,
            retry_after,
        } => {
            let workspace = client.resolve(&root)?;
            client.defer(&service::DeferRequest {
                protocol: PROTOCOL_VERSION,
                workspace_id: workspace.workspace_id,
                connection,
                batch_id,
                lease,
                delay_seconds: retry_after,
            })?
        }
        ConnectorCommand::Respond {
            batch_id,
            connection,
            reply_id,
            body_file,
        } => {
            let body = read_reply_body(&body_file)?;
            let workspace = client.resolve(&root)?;
            client.respond(&service::RespondRequest {
                protocol: PROTOCOL_VERSION,
                workspace_id: workspace.workspace_id,
                connection,
                batch_id,
                reply_id,
                body,
            })?
        }
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn configured_root(root: Option<&Path>) -> Result<PathBuf> {
    Ok(match root {
        Some(root) => root.canonicalize()?,
        None => std::env::current_dir()?.canonicalize()?,
    })
}

fn read_reply_body(path: &Path) -> Result<String> {
    const LIMIT: u64 = 1024 * 1024;
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        io::stdin().take(LIMIT + 1).read_to_end(&mut bytes)?;
    } else {
        File::open(path)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
    }
    ensure!(bytes.len() as u64 <= LIMIT, "Agent reply exceeds 1 MiB");
    Ok(String::from_utf8(bytes)?)
}
