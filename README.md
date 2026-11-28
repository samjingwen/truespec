# TrueSpec

Review project documentation with the precision of a code review, then hand the feedback back to the coding agent already working on it.

TrueSpec is a local-first Rust CLI that serves existing Markdown in a browser, lets you annotate exact source context, collects comments across documents, and delivers durable feedback batches to coding agents.
OpenSpec is the first structured document source, and ordinary Markdown folders work through the same review core.

## Why TrueSpec

Long chat messages lose the relationship between feedback and the document text they discuss.
Screenshots preserve appearance but lose source identity.
Editing source directly mixes review comments with the Markdown that remains the source of truth.

TrueSpec keeps those concerns separate:

- Markdown stays in place and remains the source of truth.
- Comments, review sessions, delivery state, and resolution state live in a local SQLite database.
- Source-aware anchors preserve the original quote, heading context, revision, and source range.
- Several comments across several documents can be sent as one batch.
- Coding agents continue the conversation and edit files through their existing harness.
- The browser refreshes changed Markdown so another review round can use the same tab.

TrueSpec is not a replacement for OpenSpec.
It does not create a competing specification format, merge delta specs, apply proposals, complete tasks, or archive changes.
It reads OpenSpec artifacts in place and adds a review and agent-handoff layer around them.

## Features

- **OpenSpec-aware navigation:** current specifications, active changes under **Changes**, archived changes under **Archives**, and nested `delta` documents remain distinct.
- **Plain Markdown source:** point TrueSpec at any Markdown directory without installing OpenSpec.
- **Precise annotations:** target a Markdown source line, paragraph, whole list, section, or exact rendered text selection.
- **General feedback:** queue comments without a document annotation from a compact composer that can collapse without losing pending text.
- **Cross-document review feed:** drafts and submitted feedback remain in one chronological list.
- **Conservative re-anchoring:** reliable unique matches relocate after edits; ambiguous targets become **Outdated** instead of attaching to the wrong text.
- **Durable batching:** feedback is committed before the browser reports success and remains queued while no agent is connected.
- **OpenCode integration:** `/truespec-review` pairs the current chat and worktree with one browser review.
- **Shared local service:** one authenticated user-scoped daemon serves concurrent reviews from several initialized repositories.
- **Private tailnet access:** optionally open the same review from another Tailscale device while the core service, source files, review state, and coding agent remain on the host.
- **Agent reply mirror:** one completed harness response is appended to the browser feed for each batch.
- **Harness-independent connector protocol:** future coding harnesses can use the same local delivery contract.
- **Responsive workspace:** browse adapter-derived recursive document folders with remembered disclosure state, collapse either desktop panel, resize Review to a persistent width, expand it for focused feedback, and use light-dismiss drawers on smaller screens.
- **Single binary:** the web interface and SQLite are embedded in the Rust executable.

## How it works

```text
OpenSpec or Markdown files
          |
          v
  DocumentSource adapter
          |
          v
Markdown renderer + source map
          |
          v
Annotations, drafts, batches, reviews
          |
          v
Repository-owned SQLite inbox
           |
           v
 Shared loopback service
           |
           v
 Selected integration adapter
           |
           v
   truespec-review skill
           |
           v
 Coding agent edits Markdown
          |
          v
 Browser refreshes files
```

Document-source adapters and coding-harness connectors are independent.
The shared review core does not require OpenSpec or OpenCode.

## Installation

### Requirements

- A current stable Rust toolchain.
- A C compiler for the bundled SQLite build.
- A Git repository root for project initialization; TrueSpec does not invoke the Git executable.
- macOS or Linux for the currently verified delivery paths.

Install directly from the public repository:

```sh
cargo install \
  --git https://github.com/samjingwen/truespec.git \
  --locked
```

Upgrade an existing installation:

```sh
cargo install \
  --git https://github.com/samjingwen/truespec.git \
  --locked \
  --force
```

Verify:

```sh
truespec --version
```

The installed binary does not require Node.js, OpenSpec, Lavish, or a separate frontend build.
Tailscale is optional and is used only when private tailnet access is requested.

## Initialize a repository

Run initialization once from the Git repository root:

```sh
truespec init
```

TrueSpec detects `openspec/` when present and otherwise selects Markdown from the repository root.
The interactive flow presents a multi-select for one or more coding integrations.
Use source flags when you want to override automatic detection.
OpenCode is currently the only built-in integration.

For scripts and automated environments, select integrations explicitly:

```sh
truespec init --integration opencode
truespec init --all
```

Choose a plain Markdown source explicitly:

```sh
truespec init \
  --source markdown \
  --source-path documentation \
  --integration opencode
```

`--source-path` is relative to the repository root.
It defaults to `openspec` for OpenSpec and `.` for Markdown.
Repeat `--integration` to install several integrations when more become available.

Initialization writes `truespec/config/project.json`, preserves `truespec/.state/`, and installs each selected integration for the current user.
It also starts or reuses the shared TrueSpec service and registers the repository's stable local workspace identity.
OpenSpec is not a runtime dependency.
TrueSpec discovers and reads its files directly.

## Review workflow

### 1. Read and navigate

The Documents panel links current specs and change artifacts without merging their contents.
Relative Markdown links and heading fragments navigate inside the viewer.
Turn **Annotate** off when you want every click to behave as ordinary document navigation.

### 2. Add contextual comments

- Hover over text to target one Markdown source line, including all of its visual wraps.
- Use the inner left margin to target a paragraph or whole ordered or unordered list.
- Use the outer left margin to target a heading and its complete section.
- Drag across rendered text for an exact selection, then use **Comment on selection**.
- Type into **Add a general comment...** for feedback without a source annotation.

Comments can be questions, clarification requests, suggestions, or explicit change requests.
The agent should answer questions without editing Markdown merely because an explanation was requested.
If the intent is ambiguous, it should ask in the coding chat before changing files.
If a question reveals a clear documentation defect, the agent may improve it and explain why.

For inline comments:

- **Enter** queues the comment.
- **Shift+Enter** inserts a newline.
- **Escape** cancels.

For general comments:

- **Enter** queues the comment.
- **Shift+Enter** inserts a newline.

Successful queue actions show **Comment queued.**
Drafts use a dashed outline and remain editable or removable.

### 3. Send a batch

- **Send** submits every queued draft and keeps the review open.
- **Send and End** submits queued drafts and ends the browser review.
- With no drafts, **Send and End** only ends the review.

Submitted items remain in their chronological positions.
Delivery shows a labeled spinner while the agent is waiting or working and a green check after the agent replies or records completion.
Source changes add **Outdated** or source-missing warnings independently from agent progress.
TrueSpec retains legacy resolution data in existing local databases but does not expose reviewer-resolution controls or badges.

### 4. Review updated Markdown

TrueSpec checks documents for external edits.
Changed content appears in the same browser review without moving comments silently.
Reliable targets relocate; ambiguous or missing targets retain their original context and require review.

## OpenCode workflow

### Initialize OpenCode

From the repository root:

```sh
truespec init --integration opencode
```

Initialization installs the OpenCode plugin and portable `truespec-review` skill in OpenCode's global discovery directories.
The generated plugin records the installed TrueSpec binary path, so the OpenCode service does not need to inherit Cargo's PATH.
Matching generated files are preserved.
A differing plugin or skill is protected unless `--force` is explicit.

OpenCode watches its global plugin and skill directories.
Run `/reload` if the command or skill is not immediately visible.

### Start a review

In an OpenCode V2 chat located at the initialized repository root:

```text
/truespec-review
```

The command takes no arguments.
It reads `truespec/config/project.json`, ensures the shared service is healthy, and opens or resumes one workspace-scoped review paired with that exact OpenCode chat and worktree.
Several initialized repositories can keep independent reviews open through the same service.
It does not open a browser automatically.
Its response contains only the available review URL lines.
Local mode reports the loopback URL once.
Tailnet mode reports the tailnet URL first and the loopback URL second.

```text
https://device.example.ts.net:8443/w/<workspace>/?review=<review>
http://127.0.0.1:47371/w/<workspace>/?review=<review>
```

The iterative loop is:

1. Add comments in TrueSpec.
2. Click **Send**.
3. The batch appears as one concise `@truespec-review` message in the same OpenCode chat.
4. The skill guides the agent to answer questions or apply clearly requested Markdown changes in place.
5. The completed OpenCode response is mirrored into the chronological Review feed.
6. TrueSpec refreshes changed Markdown in the same browser tab.
7. Repeat until the review is complete.
8. Click **Send and End**.

Sending does not interrupt a running OpenCode turn.
It queues the review batch in the session inbox.
Ending TrueSpec does not close the OpenCode chat.
OpenCode remains the canonical conversation; the browser reply is a read-only mirror linked to that batch.
An agent reply shows the green done indicator.

### Update an installed integration

Re-run initialization after upgrading the binary:

```sh
truespec init --integration opencode --force
```

Use `--force` only after confirming that you intend to replace a differing generated plugin or skill.
TrueSpec preflights the complete initialization plan before writing project or integration files.

## Review from another tailnet device

TrueSpec can use [Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve) to make browser review routes available to other devices on the same tailnet.
The TrueSpec service still listens only on `127.0.0.1`.
Tailscale terminates private HTTPS and proxies browser traffic to that loopback service.

Before starting:

1. Install Tailscale and sign in on the TrueSpec host and the reviewing device.
2. Ensure the tailnet access policy allows the reviewer to reach the host on the selected HTTPS port.
3. Enable HTTPS certificates for the tailnet and ensure the host has a Tailscale DNS name.

Start TrueSpec with the dedicated default HTTPS port:

```sh
truespec up --tailnet
```

If port `8443` is already used by another Serve configuration, select another dedicated port:

```sh
truespec up --tailnet --tailnet-port 9443
```

TrueSpec refuses to replace or adopt an existing listener.
Do not modify TrueSpec's selected Serve port concurrently with `truespec up` or `truespec down`.
If Tailscale times out while provisioning HTTPS, enable tailnet HTTPS certificates in the Tailscale admin console and retry.
On success, status reports the private `https://<device>.<tailnet>.ts.net:<port>` origin.
Run `/truespec-review` normally and the integration opens the tailnet URL.
Existing loopback review URLs continue to work on the host and refer to the same review.

Tailnet mode remains enabled across automatic service recovery.
Running `truespec down` removes only the exact Serve listener recorded by TrueSpec and disables tailnet mode.
The next ordinary `truespec up` starts in local-only mode.
If the recorded listener is already absent, `down` treats external cleanup as complete and clears the stale ownership record.
If the listener no longer matches the recorded target, `down` leaves it unchanged and prints an exact-port inspection and recovery command.
The local TrueSpec service still stops when Serve cleanup cannot be verified.

Remote browser requests must arrive through the exact configured HTTPS origin and include a Tailscale user identity.
Traffic without a user identity, including traffic from tagged devices that does not carry user identity headers, is rejected.
Tailscale access-control rules remain the network authorization boundary.
Private `/__control/*` routes are always rejected through the tailnet origin.
The browser never receives the local bearer credential.

Source Markdown, repository review databases, connector delivery, coding-agent execution, and provider credentials remain on the TrueSpec host.
TrueSpec does not store Tailscale user identities.

## Command reference

### `truespec init`

Initializes the current repository and installs coding integrations.

```text
truespec init [--source <ID>] [--source-path <PATH>]
              [--integration <ID>]... [--all] [--force] [--reset-state]
```

| Option | Meaning |
| --- | --- |
| `--source <ID>` | Select a registered source adapter. Current values are `openspec` and `markdown`. |
| `--source-path <PATH>` | Select the adapter root relative to the repository. |
| `--integration <ID>` | Install one coding integration. Repeat to select several. |
| `--all` | Install every registered integration. |
| `--force` | Replace differing generated files for selected integrations. |
| `--reset-state` | Permanently discard repository-local review state and create a fresh workspace identity. |

With no integration flags in an interactive terminal, initialization presents a multi-select.
Non-interactive initialization requires `--integration` or `--all`.
Run `truespec --help` for the authoritative public command surface.

Connector transport and viewer operations are hidden implementation details.
Their accepted behavior is defined by the [connector protocol specification](openspec/specs/connector-protocol/spec.md).

### `truespec up`

Starts or reuses the user-scoped background service and waits for authenticated readiness.
It can run outside a repository.

```text
truespec up [--tailnet] [--tailnet-port <PORT>] [--json]
```

`--tailnet` configures one private Tailscale Serve HTTPS listener on port `8443`.
`--tailnet-port` selects another dedicated HTTPS port and requires `--tailnet`.

### `truespec status`

Reports service state, version, loopback endpoint, process identity, registered-project count, tailnet state, and the open-review count when all repository state is loaded.
Tailnet status includes only the configured HTTPS port, private origin, and whether the owned Serve listener still matches.
Otherwise human output says the review count is unknown and JSON returns `"open_reviews": null`.

```text
truespec status [--json]
```

### `truespec down`

Requests checked graceful shutdown of the matching authenticated service.
When tailnet mode is enabled, it removes only the exact TrueSpec-owned Serve listener and then disables the persisted mode.
It is safe to repeat when the service is already stopped.

```text
truespec down [--json]
```

### Upgrade from the pre-initialization MVP

Run `truespec init` once from each repository root after upgrading.
Existing initialized repositories must refresh the OpenCode integration after this shared-service protocol change:

```sh
truespec init --integration opencode --force
```

Review databases from older schema versions are not migrated automatically.
Normal initialization refuses an unsupported database without modifying it.
To intentionally discard drafts, review history, batches, replies, and connector bindings, run:

```sh
truespec init --integration opencode --force --reset-state
```

`--reset-state` removes only a validated repository-local `truespec/.state/` directory.
It preserves source Markdown and `truespec/config/project.json`, then creates a fresh database and workspace ID.
The shared local service drains and restarts around deletion, briefly disconnecting other open local reviews without changing their repository state.
For a supported current database, the first selected integration adopts outstanding connector-free batches without changing their snapshots or completed items.
The former public viewer, setup, manual inbox, reply, and relink commands are no longer available.

## Storage and state

Shared source configuration lives at:

```text
truespec/config/project.json
```

Commit this file so contributors review the same document source.
Integration selection remains user-local.

Review state lives at:

```text
truespec/.state/review.sqlite3
```

Keep `truespec/.state/` out of version control.
The database stores document identities, anchors, drafts, immutable batch snapshots, leases, connector pairings, delivery receipts, and reviewer resolution state.
It also stores one random workspace ID for that physical checkout and its local state.
A fresh clone without copied state receives a different workspace ID.

Markdown content is never copied into a TrueSpec-owned documentation tree.
Submitted batch snapshots preserve exactly what the agent received even when the source changes later.

User-scoped service files use the platform's XDG directories, with home-directory fallbacks:

```text
$XDG_CONFIG_HOME/truespec/service.json
$XDG_STATE_HOME/truespec/service.json
$XDG_STATE_HOME/truespec/service.log
$XDG_DATA_HOME/truespec/registry.sqlite3
```

The private configuration contains the local control credential, preferred local port, and optional exact tailnet origin, HTTPS port, and loopback proxy target.
The runtime registration contains process and service discovery identity.
The global registry contains only workspace ID, canonical root, configuration fingerprint, and availability metadata.
Documents, comments, anchors, reviews, batches, receipts, replies, and source snapshots remain in each repository's `truespec/.state/review.sqlite3`.
Deleting the recoverable global registry does not delete review history; reinitializing or reattaching a repository rebuilds its route from repository-owned identity.

## Security and privacy

- The core server always binds to IPv4 loopback.
- Optional tailnet access uses Tailscale Serve for private HTTPS while preserving the loopback listener.
- Tailnet browser traffic requires the exact configured Host and Origin plus a non-empty Tailscale user identity.
- Tailnet access remains subject to the tailnet access policy.
- Private `/__control/*` routes remain loopback-only and are never exposed through Serve.
- Private lifecycle, registration, and connector control routes require a random owner-only bearer credential.
- Service configuration, runtime registration, routing metadata, and bounded logs use owner-only directories and files.
- Mutating HTTP requests require the expected same-origin browser origin.
- Browser routes use opaque workspace and review identities and never accept a filesystem path as routing authority.
- Document routes expose only discovered Markdown documents.
- `truespec/.state/` and `truespec/config/` are excluded from Markdown discovery.
- Symlinks cannot escape a selected document tree.
- Raw Markdown HTML and scripts do not execute.
- External images are not loaded; image alt text remains readable.
- Safe `http`, `https`, and `mailto` links are allowed.
- The browser and core contain no model credentials or model runtime.
- Global logs omit source text, review bodies, connector destinations, control credentials, project paths, Tailscale identities, private DNS names, and complete review URLs.

## Architecture

TrueSpec is one Rust binary with ordinary internal boundaries:

| Area | Responsibility |
| --- | --- |
| `source` | Source-adapter registry, document-source interface, OpenSpec grouping, and Markdown-folder traversal. |
| `project` | Versioned project configuration, source validation, and repository-root checks. |
| `initialize` | Multi-integration selection, complete preflight, and atomic generated-file installation. |
| `integration` | Coding-harness adapter registry and generated plugin and skill artifacts. |
| `markdown` | Safe GFM rendering, heading fragments, rendered-text projection, and source offset maps. |
| `anchor` | Line, paragraph, list, section, selection, and general-feedback identity and relocation. |
| `review` | SQLite schema, document identities, drafts, sessions, immutable batch snapshots, and resolution. |
| `inbox` | Connector leasing, receipts, retries, and delivery envelopes. |
| `service` | XDG paths, private configuration, runtime registration, authenticated lifecycle client, detached ownership, and bounded logs. |
| `tailnet` | Bounded Tailscale discovery, private DNS validation, exact Serve listener ownership, and remote readiness checks. |
| `service_registry` | Minimal global workspace routing metadata and lazy validated project contexts. |
| `web` | Shared browser routes, exact local or tailnet request trust, loopback-only control APIs, and embedded frontend assets. |
| `skills/truespec-review` | Portable instructions for handling connector-delivered review feedback. |

The browser assets in `web/` are embedded with `include_str!`.
There is no generated frontend directory to deploy.
Rendered document HTML stays in memory.

The connector boundary is defined by the [connector protocol specification](openspec/specs/connector-protocol/spec.md).
OpenCode is one integration adapter over that protocol, not a dependency of the review core.

## Current boundaries

The MVP intentionally does not provide:

- Browser-based source editing.
- Hosted accounts or cloud synchronization.
- Multi-user collaboration.
- Hosted synchronization, arbitrary tunnel ingress, or LAN exposure outside the configured tailnet route.
- Operating-system service installation such as `launchd`.
- Automatic launching of arbitrary coding agents.
- A dynamic connector marketplace.
- Exactly-once guarantees for edits performed outside TrueSpec.
- Non-Markdown document rendering.

Documents must be valid UTF-8 Markdown and no larger than 2 MiB.
TrueSpec is currently verified on macOS and Linux.

## Development

Every repository edit is developed on a dedicated Worktrunk branch and worktree.
Keep the change scoped to its active OpenSpec change, preserve unrelated work, and do not edit directly on `master`.

Run the ordinary quality gate:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
openspec validate --all --strict
```

Run the real-browser flow for both document adapters:

```sh
cargo build --locked
node tests/browser.mjs
```

The browser test requires Node.js and `chrome-devtools-axi`.
It covers desktop and mobile layouts, annotation targets, unified review state, batching, shared-service restart persistence, two-repository isolation, delivery, drawers, and agent handoff.

Run the isolated OpenCode integration:

```sh
node tests/opencode.mjs
```

That test starts an isolated OpenCode V2 server and deterministic local model fixture.
It verifies concurrent chats from two repositories, isolated deliveries and replies, daemon recovery, stable message identities, and Markdown updates at each workspace-scoped TrueSpec URL.

Before delivery, review the complete base-to-head diff and run the installed Hardlook workflow for implementation changes.
Push the feature branch, open a GitHub pull request, and merge only after every conversation is resolved and both required hosted checks pass:

- `Quality` runs formatting, Clippy, Rust tests, and strict OpenSpec validation.
- `Browser and OpenCode` builds the binary and runs both Node integration suites.

After the GitHub merge, fast-forward the clean primary `master` checkout, verify that it equals `origin/master` and contains the merge, then return the coding-harness session with a verified native workspace transition when one is available.
Release and deployment operations require separate explicit authorization.

CI uses Rust 1.95.0 and Node.js 26.13.1 with OpenSpec 2.1.0, OpenCode 2.4.2, `chrome-devtools-axi` 0.1.46, and `chrome-devtools-mcp` 1.14.0.
These are development tools and remain outside the TrueSpec runtime.

TrueSpec itself is developed with OpenSpec.
Accepted behavior is under [`openspec/specs/`](openspec/specs/).
The completed MVP proposal, design, tasks, and decisions are preserved in the [`build-truespec-mvp` archive](openspec/changes/archive/2026-11-21-build-truespec-mvp/).

## Inspiration

TrueSpec takes inspiration from Lavish's contextual annotation and agent feedback loop.
Lavish is not a runtime dependency, and TrueSpec does not adopt Lavish's design workflow or visual design.

## License

TrueSpec is available under the [MIT License](LICENSE).
