mod anchor;
mod inbox;
mod markdown;
mod review;
mod setup;
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
use inbox::{AgentInbox, LocalInbox, PROTOCOL_VERSION};
use review::Store;
use source::SourceKind;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Workspace root. Agent commands otherwise search the working directory and its parents.
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the local browser viewer.
    Serve {
        #[arg(long, value_enum, default_value = "openspec")]
        source: SourceKind,
        /// Document directory, relative to or inside the workspace.
        #[arg(long)]
        docs: Option<PathBuf>,
        /// Use an explicit loopback port. Zero asks the OS to choose.
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Emit a versioned JSON startup record instead of human-readable lines.
        #[arg(long)]
        json: bool,
        /// Delete this workspace's local TrueSpec state before starting.
        #[arg(long)]
        reset_state: bool,
    },
    /// Listen for manual inbox work until activity or an inactivity timeout.
    Listen {
        #[arg(long)]
        session: String,
        /// Exit after this period without a batch or review end.
        #[arg(long, default_value = "30m", value_parser = parse_idle_timeout)]
        idle_timeout: Duration,
        /// Emit compact JSON on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Acknowledge receipt and renew the delivery lease.
    Ack {
        batch_id: String,
        #[arg(long)]
        lease: String,
    },
    /// Record completion of one item without posting a browser reply.
    Complete {
        batch_id: String,
        comment_id: String,
        #[arg(long)]
        lease: String,
    },
    /// Store one final Markdown reply for a manually delivered batch.
    Reply {
        batch_id: String,
        /// Read Markdown from this file, or use - for stdin.
        #[arg(long)]
        body_file: PathBuf,
        /// Stable retry key. Defaults to one deterministic ID per batch.
        #[arg(long)]
        reply_id: Option<String>,
    },
    /// Associate a moved Markdown file with its existing annotation history.
    Relink {
        old_path: PathBuf,
        new_path: PathBuf,
    },
    /// Pair and deliver feedback through a local coding harness connector.
    Connector {
        #[arg(long, default_value_t = PROTOCOL_VERSION)]
        protocol: u8,
        #[command(subcommand)]
        command: ConnectorCommand,
    },
    /// Install optional coding-harness integrations.
    Setup {
        /// Coding harness connector to install. Repeat to install several.
        #[arg(long, value_enum, conflicts_with_all = ["all", "list"])]
        connector: Vec<setup::Connector>,
        /// Install every supported connector.
        #[arg(long, conflicts_with_all = ["connector", "list"])]
        all: bool,
        /// Show supported connectors and installation state without changing files.
        #[arg(long, conflicts_with_all = ["connector", "all", "force"])]
        list: bool,
        /// Replace a plugin file that differs from TrueSpec's generated copy.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ConnectorCommand {
    /// Pair with the open review or start a new review if the previous one ended.
    Attach {
        /// Optional document source to validate and use for workspace discovery.
        #[arg(long, value_enum)]
        source: Option<SourceKind>,
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
    let Cli { root, command } = Cli::parse();
    let root = root.as_deref();
    match command {
        Command::Serve {
            source: kind,
            docs,
            port,
            json,
            reset_state,
        } => {
            let root =
                source::workspace(root, matches!(kind, SourceKind::OpenSpec) && docs.is_none())?;
            let adapter = source::open(&root, docs.as_deref(), kind)?;
            if reset_state {
                review::reset_state(&root)?;
            }
            let store = Store::open(&root, true)?;
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(web::serve(store, adapter, root, kind.name(), port, json))
        }
        Command::Listen {
            session,
            idle_timeout,
            json,
        } => {
            let inbox = LocalInbox(Store::open(&review::agent_root(root)?, false)?);
            let envelope = inbox.listen(&session, idle_timeout)?;
            let output = if json {
                serde_json::to_string(&envelope)?
            } else {
                serde_json::to_string_pretty(&envelope)?
            };
            println!("{output}");
            Ok(())
        }
        Command::Ack { batch_id, lease } => {
            let inbox = LocalInbox(Store::open(&review::agent_root(root)?, false)?);
            let expires = inbox.acknowledge(&batch_id, &lease)?;
            println!("Receipt recorded. Lease expires at {expires} Unix milliseconds.");
            Ok(())
        }
        Command::Complete {
            batch_id,
            comment_id,
            lease,
        } => {
            let inbox = LocalInbox(Store::open(&review::agent_root(root)?, false)?);
            inbox.complete(&batch_id, &comment_id, &lease)?;
            println!("Completed {comment_id}.");
            Ok(())
        }
        Command::Reply {
            batch_id,
            body_file,
            reply_id,
        } => {
            let body = read_reply_body(&body_file)?;
            let reply_id = reply_id.unwrap_or_else(|| format!("manual:{batch_id}"));
            let store = Store::open(&review::agent_root(root)?, false)?;
            store.record_manual_reply(&batch_id, &reply_id, &body)?;
            println!("Stored reply for {batch_id}.");
            Ok(())
        }
        Command::Relink { old_path, new_path } => {
            let root = review::agent_root(root)?;
            source::check_relative(&old_path)?;
            match root.join(&old_path).symlink_metadata() {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
                Ok(_) => ensure!(
                    false,
                    "The old source still exists. Only missing paths can be relinked."
                ),
            }
            let from = source::as_locator(&old_path)?;
            let to = source::existing_locator(&root, &new_path)?;
            let id = Store::open(&root, false)?.relink(&from, &to)?;
            println!("Relinked {id} from {from} to {to}.");
            Ok(())
        }
        Command::Connector { protocol, command } => {
            ensure!(
                protocol == PROTOCOL_VERSION,
                "Unsupported connector protocol {protocol}; expected {PROTOCOL_VERSION}."
            );
            connector(root, command)
        }
        Command::Setup {
            connector,
            all,
            list,
            force,
        } => setup::run(connector, all, list, force),
    }
}

fn connector(root: Option<&std::path::Path>, command: ConnectorCommand) -> Result<()> {
    let output = match command {
        ConnectorCommand::Attach {
            source: kind,
            kind: name,
            destination,
            label,
        } => {
            let root = source::workspace(root, matches!(kind, Some(SourceKind::OpenSpec)))?;
            if let Some(kind) = kind {
                source::open(&root, None, kind)?;
            }
            let binding = Store::open(&root, true)?.bind_connector(&name, &destination, &label)?;
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "workspace": root,
                "connection_id": binding.connection_id,
                "review_id": binding.review_id,
            })
        }
        ConnectorCommand::Poll { connection, wait } => {
            let inbox = LocalInbox(Store::open(&review::agent_root(root)?, false)?);
            serde_json::to_value(inbox.poll_connector(&connection, Duration::from_secs(wait))?)?
        }
        ConnectorCommand::Delivered {
            batch_id,
            connection,
            lease,
            receipt,
        } => {
            let inbox = LocalInbox(Store::open(&review::agent_root(root)?, false)?);
            inbox.delivered(&connection, &batch_id, &lease, &receipt)?;
            serde_json::json!({"protocol_version": PROTOCOL_VERSION, "event": "delivered"})
        }
        ConnectorCommand::Defer {
            batch_id,
            connection,
            lease,
            retry_after,
        } => {
            let inbox = LocalInbox(Store::open(&review::agent_root(root)?, false)?);
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
            let store = Store::open(&review::agent_root(root)?, false)?;
            store.record_reply(&connection, &batch_id, &reply_id, &body)?;
            serde_json::json!({"protocol_version": PROTOCOL_VERSION, "event": "responded"})
        }
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
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

fn parse_idle_timeout(value: &str) -> Result<Duration, String> {
    let duration = humantime::parse_duration(value).map_err(|error| error.to_string())?;
    if duration.is_zero() || duration > Duration::from_secs(24 * 60 * 60) {
        return Err("duration must be greater than zero and no more than 24h".into());
    }
    Ok(duration)
}
