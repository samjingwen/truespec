mod anchor;
mod inbox;
mod initialize;
mod integration;
mod markdown;
mod project;
mod review;
mod source;
mod web;

use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use inbox::{LocalInbox, PROTOCOL_VERSION};
use review::Store;

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
enum ConnectorCommand {
    /// Start the initialized loopback viewer.
    Serve {
        /// Use an explicit loopback port. Zero asks the OS to choose.
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Emit a versioned JSON startup record.
        #[arg(long)]
        json: bool,
        /// Delete only local review state before starting.
        #[arg(long)]
        reset_state: bool,
    },
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
            let changed =
                initialize::InitPlan::build(&root, &config, &integrations, &context, force)?
                    .apply()?;
            println!("Initialized TrueSpec in {}", root.display());
            println!(
                "Document source: {} ({})",
                config.adapter().label(),
                config.source_path().display()
            );
            for path in changed {
                println!("Installed {}", path.display());
            }
            Ok(())
        }
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

fn connector(root: Option<&std::path::Path>, command: ConnectorCommand) -> Result<()> {
    if let ConnectorCommand::Serve {
        port,
        json,
        reset_state,
    } = command
    {
        let (root, config) = configured_project(root)?;
        let adapter = config.adapter().open(&root, config.source_path())?;
        if reset_state {
            review::reset_state(&root)?;
        }
        let store = Store::open(&root, true)?;
        return tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(web::serve(
                store,
                adapter,
                root,
                config.adapter().label(),
                port,
                json,
            ));
    }
    let (root, config) = configured_project(root)?;
    let output = match command {
        ConnectorCommand::Attach {
            kind: name,
            destination,
            label,
        } => {
            config.adapter().open(&root, config.source_path())?;
            let binding = Store::open(&root, true)?.bind_connector(&name, &destination, &label)?;
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "workspace": root,
                "connection_id": binding.connection_id,
                "review_id": binding.review_id,
            })
        }
        ConnectorCommand::Poll { connection, wait } => {
            let inbox = LocalInbox(Store::open(&root, false)?);
            serde_json::to_value(inbox.poll_connector(&connection, Duration::from_secs(wait))?)?
        }
        ConnectorCommand::Delivered {
            batch_id,
            connection,
            lease,
            receipt,
        } => {
            let inbox = LocalInbox(Store::open(&root, false)?);
            inbox.delivered(&connection, &batch_id, &lease, &receipt)?;
            serde_json::json!({"protocol_version": PROTOCOL_VERSION, "event": "delivered"})
        }
        ConnectorCommand::Defer {
            batch_id,
            connection,
            lease,
            retry_after,
        } => {
            let inbox = LocalInbox(Store::open(&root, false)?);
            inbox.defer(&connection, &batch_id, &lease, retry_after)?;
            serde_json::json!({"protocol_version": PROTOCOL_VERSION, "event": "deferred"})
        }
        ConnectorCommand::Respond {
            batch_id,
            connection,
            reply_id,
            body_file,
        } => {
            let body = read_reply_body(&body_file)?;
            let store = Store::open(&root, false)?;
            store.record_reply(&connection, &batch_id, &reply_id, &body)?;
            serde_json::json!({"protocol_version": PROTOCOL_VERSION, "event": "responded"})
        }
        ConnectorCommand::Serve { .. } => unreachable!(),
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn configured_project(root: Option<&Path>) -> Result<(PathBuf, project::ProjectConfig)> {
    let root = match root {
        Some(root) => root.canonicalize()?,
        None => std::env::current_dir()?.canonicalize()?,
    };
    let config = project::ProjectConfig::load(&root)?;
    Ok((root, config))
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
