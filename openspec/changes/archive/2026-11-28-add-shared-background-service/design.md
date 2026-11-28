# Design

## Context

See `proposal.md` for motivation.
TrueSpec currently combines a repository-scoped SQLite store, one source adapter, one Axum listener, and one generated OpenCode plugin viewer child.
Each `/truespec-review` invocation can therefore own only the workspace that started its process.
The existing browser API trusts loopback plus Host and Origin checks, connector commands access repository state directly, and public help exposes only `init`.
The selected first milestone is local-only TrueSpec development on one machine; TenAxe integration, Mac Mini public ingress, accounts, hosted snapshots, and multi-user review remain later work.

## Goals / Non-Goals

**Goals:**

- Run one authenticated user-scoped service that can host concurrent reviews from multiple initialized repositories.
- Make service startup, discovery, status, compatibility, shutdown, and stale recovery explicit and testable.
- Preserve repository-owned source configuration and review databases.
- Keep browser and connector requests workspace-scoped without accepting browser-selected filesystem paths.
- Preserve current annotation, batching, delivery, reply, and source-refresh behavior.
- Keep a later bounded TenAxe adapter possible without adding TenAxe dependencies or changing TenAxe now.

**Non-Goals:**

- Publish TrueSpec to a LAN or the Internet.
- Add reviewer accounts, expiring remote links, collaboration, hosted persistence, or offline review.
- Add Postgres, Redis, NATS, Caddy, Cloudflare Tunnel, Tailscale, containers, or launchd installation in this milestone.
- Move coding-agent execution, provider credentials, or conversation ownership into TrueSpec.
- Identify one logical repository across independent clones and machines.

## Decisions

### Keep the existing Rust stack and add a local service client boundary

Continue using Rust, Tokio, Axum, Clap, bundled SQLite through `rusqlite`, Serde, and UUIDs.
Add a bounded local HTTP client for authenticated CLI-to-service calls, cryptographically secure random control credentials, constant-time credential checks, owner-only file locking, and structured bounded logging.
Do not add a second database server, message broker, frontend toolchain, or runtime language.

SQLite WAL remains appropriate because each database stays on the same local filesystem and the expected workload has one service writer with concurrent reads.
Use short transactions, `busy_timeout`, foreign keys, and the existing full synchronous policy.
The global registry and project databases must never live on a network filesystem or cloud-synchronized directory.

### Separate persistent configuration, runtime registration, and routing data

Resolve global directories through XDG variables with deterministic user-home fallbacks:

```text
config/truespec/service.json     persistent preferred port and private control credential
state/truespec/service.json      atomic runtime registration
state/truespec/service.log       bounded operational log
data/truespec/registry.sqlite3   recoverable workspace routing metadata
```

Create directories with owner-only permissions and sensitive files with mode `0600`.
The persistent service config keeps one random 256-bit control credential and one selected loopback port across restarts.
The runtime registration contains an instance UUID, PID, service version, protocol version, loopback URL, startup state, and registration generation, but no project state or review content.
The registry stores workspace ID, canonical root, configuration fingerprint, and availability metadata only.

Default status output reports safe counts and service metadata without project paths, credentials, connector destinations, review bodies, or complete browser URLs.
Machine-readable or verbose diagnostics must still redact credentials and review content.

### Represent a physical state-bearing checkout with a workspace ID

Add an append-only project database migration that stores one random workspace UUID in the repository-owned review database.
The workspace ID represents one checkout plus its `truespec/.state` contents, not a logical repository shared by every clone.

- Moving the checkout with its state preserves identity.
- A fresh clone without state receives a new identity.
- Two available roots presenting copied state and the same workspace ID are a conflict.
- Rebinding after a move is allowed only when the old canonical root is unavailable and the new root validates.

The global registry caches `workspace ID -> canonical root`.
If the registry is lost, attaching or reinitializing a repository reconstructs its mapping from repository-owned identity without losing review history.
A future TenAxe project ID can map to this workspace ID without becoming part of TrueSpec's project state.

### Run one lazy project context per registered workspace

Replace the single `web::App` store/source/workspace fields with a service registry that resolves `workspace_id` to a lazily loaded project context:

```text
ProjectContext
  canonical root
  validated ProjectConfig
  repository Store
  opened DocumentSource
  configuration fingerprint
```

The service validates a project on registration and again after restart, cache eviction, or an observed configuration/source change.
Missing or invalid roots become unavailable and never fall through to another project.
Evicting a context drops only memory; it never resets or deletes repository state.

Each project database retains its one-open-review constraint.
Because projects use separate databases, several workspaces can keep one review open concurrently.

### Scope browser APIs by workspace and review identity

Serve embedded assets once from the shared loopback origin.
Use browser routes shaped around opaque workspace identity, for example:

```text
/w/{workspace_id}/?review={review_id}
/api/workspaces/{workspace_id}/documents
/api/workspaces/{workspace_id}/documents/{document_id}
/api/workspaces/{workspace_id}/reviews/{review_id}
```

The browser never sends a filesystem path as authority.
Each handler resolves and validates one project context before reading source or state.
Review IDs, document IDs, connector IDs, and batch IDs remain owned by their project database.
The workspace ID is routing identity, not a secret or sufficient authorization.

Retain loopback binding, exact Host checks, mutation Origin checks, restrictive security headers, safe Markdown rendering, source-size limits, and symlink containment.
The local browser does not receive the global control credential.
Remote authentication and review-link capabilities belong to a later public-gateway change.

### Use a private authenticated control plane on the same listener

Expose service health, registration, shutdown, and connector operations only under a control route set that requires the persistent bearer credential.
Do not enable CORS for control routes.
Browser routes remain separate and cannot invoke project registration, lifecycle, arbitrary roots, connector destinations, or daemon shutdown.

The CLI reads credentials from owner-only global configuration and sends bounded requests with explicit deadlines.
Installed integrations invoke hidden CLI connector operations rather than reading global credentials themselves.
This keeps generated plugins free of service credentials and centralizes compatibility checks.

### Model service startup as a contested ownership operation

Add public `up`, `status`, and `down` commands and a hidden internal service-run command.
`up` is independent of the current repository.
It reads persistent configuration, probes the registered service, and either reuses one compatible owner or starts detached contenders.

The child runs in a separate process group with null standard input, bounded log output, a sanitized environment, and no dependence on the launching terminal.
It acquires one owner lock before binding or publishing registration.
The winner binds the configured loopback port, verifies that it owns the endpoint, writes registration atomically, and reports authenticated readiness.
Losing contenders exit cleanly.

`status` never starts a process.
It classifies stopped, starting, ready, failed, stale, incompatible, and endpoint-collision states from registration plus bounded authenticated probes.
The health response includes protocol, service version, PID, instance ID, and safe counts.

Do not silently replace an incompatible healthy service.
Report an explicit `truespec down` then `truespec up` recovery so active reviews are not killed by an upgrade check.

### Stop only a verified matching owner

`down` uses the private control API to request graceful shutdown and waits within a fixed deadline.
The daemon stops accepting new work, drains bounded in-flight requests, closes listeners, checkpoints global registry state, and removes registration only when it still names that instance.
Project databases remain authoritative and do not require a shutdown rewrite.

If the registered service is unresponsive, the client may signal only the process identity tied to the unchanged registration and verified executable ownership.
It waits after termination, escalates only within a bounded policy, and never signals a PID after the registration changes hands.
An unknown process occupying the endpoint is reported and left untouched.

### Make initialization service-aware without weakening preflight

`truespec init` keeps the current complete file preflight.
After preflight succeeds, it ensures a compatible service is healthy before applying project or integration files.
If startup fails, no repository file is written.

After file installation, init creates or opens project state, ensures the workspace ID, and registers the validated canonical root.
Registration failure leaves matching project files intact but returns a clear incomplete-registration result.
Repeating init repairs registration idempotently.
Running init outside the repository root does not start the service.

### Migrate integrations from viewer ownership to shared attachment

Advance the hidden protocol version and update generated integrations together.
The OpenCode plugin removes its workspace-to-viewer child-process map and never starts `connector serve`.
`/truespec-review` invokes the installed binary's attach operation, which ensures the service, validates and registers the chat workspace, attaches the connector, and returns the complete browser URL.

Existing connector polling, delivery receipts, defer behavior, stable message IDs, response mirroring, and ended-review drain remain unchanged at the plugin boundary.
Hidden connector commands become authenticated service clients instead of opening project SQLite directly.

An old plugin using the previous protocol fails with an actionable instruction to run:

```text
truespec init --integration opencode --force
```

Do not keep a silent fallback that starts an old per-project viewer after the shared service ships.

### Keep polling for the first local milestone

Retain the browser's existing bounded refresh polling for the first local service change.
SSE is the preferred later transport for a TenAxe or remote gateway because updates are primarily server-to-browser, but it is not required to prove local lifecycle or multi-project isolation.
Do not add WebSocket, Redis, NATS, or a durable global event log without a separate demonstrated need.

### Defer operating-system supervision and remote ingress

The first milestone validates CLI-managed detached service ownership on the current macOS and Linux targets.
Mac Mini deployment should later run the foreground daemon under `launchd`, while remote browser ingress should terminate at a separately authenticated bounded gateway.
Do not directly expose the local service or repurpose its private control credential as a remote session.

Research supports native `launchd` supervision for the Mac Mini, SQLite WAL on local storage, SSE plus HTTP for later remote updates, and no initial Postgres, broker, Caddy, or container layer.
Cloudflare Tunnel is the preferred later outbound ingress for ordinary browser links if its TLS termination and data-processing model is accepted, but no tunnel belongs in this change.

## Risks / Trade-offs

- [A long-lived service widens the impact of routing mistakes] -> Scope every request by workspace and review identity, reject browser paths, and add two-project adversarial tests.
- [A stale registration can point at a reused PID or port] -> Require matching instance, process, version, protocol, endpoint, and authenticated health before reuse, removal, or signalling.
- [Concurrent `up` or `init` commands can spawn duplicate daemons] -> Use one owner lock, bounded contenders, atomic registration, and idempotent winner discovery.
- [Copied project state duplicates a workspace ID] -> Refuse a second live root and require explicit operator resolution rather than choosing by content or recency.
- [A moved repository can look like copied state] -> Rebind only when the old root is unavailable and the new root validates the same local state.
- [Global registry loss can strand review state] -> Keep only recoverable routing metadata globally and rebuild mappings from repository-owned workspace IDs.
- [A daemon binary upgrade can break generated plugins] -> Version health and connector protocols, fail incompatibly, and require explicit integration refresh.
- [Stopping the daemon interrupts browser tabs and connector polling] -> Persist all domain state before acknowledgment, make reconnect idempotent, and report service unavailability without losing project data.
- [Detached child management differs across operating systems] -> Keep the first implementation within verified macOS/Linux process APIs and treat launchd packaging as a later deployment change.
- [Global logs can leak sensitive source or destinations] -> Use bounded structured events with safe identifiers and test redaction against paths, URLs, bodies, and credentials.
- [A global SQLite database could become an accidental state owner] -> Restrict its schema to routing and availability metadata and test that review-domain tables never appear there.

## Migration Plan

1. Archive the completed `compact-workspace-controls` change before implementation delivery.
2. Add global path resolution, private service configuration, registration schema, owner lock, authenticated health, and lifecycle client tests.
3. Add public `up`, `status`, and `down` commands plus the hidden foreground daemon entrypoint.
4. Add the append-only project workspace-identity migration and global routing registry.
5. Refactor the web server into a multi-project registry with workspace-scoped browser APIs and update the embedded client.
6. Move connector operations behind the authenticated service client and update the protocol version.
7. Replace OpenCode viewer spawning with shared-service attachment and browser URL handling.
8. Make init preflight, service ensure, file application, workspace registration, and repair behavior explicit and tested.
9. Add isolated lifecycle, stale-record, endpoint-collision, upgrade, two-project browser, two-project connector, restart, move, missing-root, and duplicate-state acceptance tests.
10. Update README, CLI help, architecture ownership, installation upgrade guidance, and generated integration instructions.

Before archive, sync all capability deltas into accepted specs.
Rollback before merge uses the previous short-lived viewer implementation.
After project database migration or generated plugin refresh, binary rollback requires compatibility review rather than assuming that replacing the executable restores the previous protocol or database behavior.

## Research Basis

- [Axum 0.8 documentation](https://docs.rs/crate/axum/latest) supports retaining the existing Tokio, Hyper, and Tower-compatible Rust service stack.
- [SQLite WAL guidance](https://sqlite.org/wal.html) supports same-host concurrent readers with one writer and rules out moving these databases onto network filesystems.
- [SQLite online backup guidance](https://sqlite.org/backup.html) informs a later backup and restore change without adding Postgres to the local milestone.
- [Apple launchd guidance](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html) supports later Mac Mini supervision of a foreground daemon rather than containerizing it.
- [WHATWG server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html) supports SSE plus HTTP as the later remote update model without requiring WebSocket now.
- [Cloudflare Tunnel documentation](https://developers.cloudflare.com/tunnel/) supports a later outbound-only Mac Mini ingress, while the current local service remains unexposed.
- [Tailscale Funnel documentation](https://tailscale.com/docs/features/tailscale-funnel) records beta, port, bandwidth, and macOS constraints that make it an alternative rather than the default external-review ingress.
- [Caddy reverse proxy documentation](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy) confirms SSE and WebSocket support but does not justify an extra proxy layer for the first local service.
