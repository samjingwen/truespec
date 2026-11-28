# Proposal

## Why

TrueSpec currently starts one short-lived viewer process per reviewed repository, which prevents one stable local backend from serving concurrent reviews across projects and gives the CLI no reliable service lifecycle.
A shared user-scoped daemon lets `truespec init`, OpenCode integrations, and future bounded consumers reuse one authenticated loopback service while every repository keeps its own source configuration and review database.

## What Changes

- Add one user-scoped TrueSpec background service that serves multiple initialized repositories concurrently over loopback.
- Add public `truespec up`, `truespec status`, and `truespec down` commands alongside `init`.
- Make `truespec init` preflight its repository changes, ensure a compatible service is healthy, initialize the repository, and register its workspace.
- Store only service discovery and recoverable workspace-routing metadata globally.
- Keep workspace identity and all review-domain state in each repository's untracked SQLite database.
- Route browser and connector operations through opaque workspace and review identities instead of browser-supplied filesystem paths.
- Replace the OpenCode plugin's per-workspace viewer child process with shared-service discovery and attachment.
- Preserve source adapters, immutable batches, connector leases, mirrored replies, and one open review per project.
- Add checked startup locking, authenticated health, version compatibility, graceful shutdown, stale-registration recovery, and bounded logs.
- Keep the first milestone local-only, without TenAxe changes, public ingress, hosted snapshots, accounts, or multi-user collaboration.
- **BREAKING**: Advance the hidden connector/viewer protocol for shared-service attachment; initialized integrations must be refreshed with `truespec init --integration <id> --force`.

## Capabilities

### New Capabilities

- `service-lifecycle`: Public CLI lifecycle, authenticated discovery, process ownership, health, compatibility, shutdown, and recovery for the shared local daemon.
- `multi-project-runtime`: Workspace identity, minimal global routing metadata, repository-owned state, lazy project contexts, and cross-project isolation.

### Modified Capabilities

- `project-initialization`: Ensure and register with the shared service and expose the expanded public CLI surface.
- `connector-protocol`: Attach integrations to workspace-scoped reviews through the shared service without per-project viewer processes.
- `document-reading`: Serve initialized project documents through one shared multi-project listener while preserving review-specific browser identity.

## Impact

- Affects CLI parsing, initialization order, process lifecycle, global path handling, SQLite migrations, service/client HTTP contracts, browser API paths, embedded assets, generated integrations, connector tests, and browser tests.
- Adds a small authenticated global registry and service registration, but does not move source Markdown, comments, anchors, batches, replies, or receipts out of project repositories.
- Keeps the eventual TenAxe adapter and Mac Mini remote ingress outside this change.
