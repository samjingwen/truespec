# TrueSpec

Review project documentation with the precision of a code review, then hand the feedback back to the coding agent already working on it.

TrueSpec is a local-first Rust CLI that serves existing Markdown in a browser, lets you annotate exact source context, collects comments across documents, and delivers durable feedback batches to coding agents.
OpenSpec is the first structured document source, and ordinary Markdown folders work through the same review core.

> **Project status:** TrueSpec is an experimental MVP.
> Its storage schema, connector protocol, and command surface may change before the first stable release.

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

- **OpenSpec-aware navigation:** current specifications, active change artifacts, nested `delta/` documents, and archived artifacts remain distinct.
- **Plain Markdown source:** point TrueSpec at any Markdown directory without installing OpenSpec.
- **Precise annotations:** target a Markdown source line, paragraph, whole list, section, or exact rendered text selection.
- **General feedback:** queue comments that have no document annotation.
- **Cross-document review feed:** drafts and submitted feedback remain in one chronological list.
- **Conservative re-anchoring:** reliable unique matches relocate after edits; ambiguous targets become **Outdated** instead of attaching to the wrong text.
- **Durable batching:** feedback is committed before the browser reports success and remains queued while no agent is connected.
- **OpenCode integration:** `/truespec-review` pairs the current chat and worktree with one browser review.
- **Agent reply mirror:** one completed harness response is appended to the browser feed for each batch.
- **Harness-independent connector protocol:** future coding harnesses can use the same local delivery contract.
- **Responsive UI:** independent desktop panels become light-dismiss drawers on smaller screens.
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
Local SQLite inbox
          |
          +-----------------------+
          |                       |
          v                       v
OpenCode connector         Manual agent loop
          |                       |
          +-----------+-----------+
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
- macOS or Linux for the currently verified delivery paths.

Clone and install:

```sh
git clone https://github.com/samjingwen/truespec.git
cd truespec
cargo install --path . --locked
```

Verify:

```sh
truespec --version
```

The installed binary does not require Node.js, OpenSpec, Lavish, or a separate frontend build.

## Quick start with OpenSpec

Run this from an OpenSpec workspace or any child directory:

```sh
truespec serve
```

TrueSpec discovers the nearest `openspec/` directory, binds only to `127.0.0.1`, and prints the browser URL and review ID.

Use an explicit workspace or port when needed:

```sh
truespec serve --root /path/to/project
truespec serve --root /path/to/project --port 4317
```

OpenSpec is not a runtime dependency.
TrueSpec discovers and reads the files directly.

## Quick start with a Markdown folder

```sh
truespec serve \
  --source markdown \
  --root /path/to/project \
  --docs docs
```

`--docs` is relative to the workspace selected by `--root`.
When omitted for the Markdown source, it defaults to the workspace itself.

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
Reviewer resolution remains a separate **Unresolved / Resolved** state.
Agent completion never resolves a comment automatically.

### 4. Review updated Markdown

TrueSpec checks documents for external edits.
Changed content appears in the same browser review without moving comments silently.
Reliable targets relocate; ambiguous or missing targets retain their original context and require review.

## OpenCode workflow

### Install the connector

Run the interactive setup:

```sh
truespec setup
```

TrueSpec lists supported coding harness connectors and their installation state.
OpenCode is currently the only built-in connector.

For scripts or non-interactive environments:

```sh
truespec setup --connector opencode
truespec setup --all
truespec setup --list
```

`--connector` is repeatable for future harnesses.
Setup is idempotent and refuses to overwrite a different connector file unless `--force` is explicit.
The generated OpenCode plugin records the installed TrueSpec binary path, so the OpenCode service does not need to inherit Cargo's PATH.

OpenCode watches its global plugin directory.
Run `/reload` if the command is not immediately visible.

### Start a paired review

In an OpenCode V2 chat located in an OpenSpec workspace:

```text
/truespec-review
```

The command takes no file argument.
It opens or resumes one TrueSpec review paired with that exact OpenCode chat and worktree.

The iterative loop is:

1. Add comments in TrueSpec.
2. Click **Send**.
3. The batch appears as one queued message in the same OpenCode chat.
4. The agent reads the current source and updates Markdown in place.
5. The completed OpenCode response is mirrored into the chronological Review feed.
6. TrueSpec refreshes changed Markdown in the same browser tab.
7. Repeat until the review is complete.
8. Click **Send and End**.

Sending does not interrupt a running OpenCode turn.
It queues the review batch in the session inbox.
Ending TrueSpec does not close the OpenCode chat.
OpenCode remains the canonical conversation; the browser reply is a read-only mirror linked to that batch.
An agent reply shows the green done indicator, but never resolves comments automatically.

### Develop with the repository-local plugin

This repository automatically exposes [`.opencode/plugins/truespec`](.opencode/plugins/truespec/) when opened in OpenCode.
Another project can reference that development plugin explicitly:

```jsonc
{
  "plugins": [
    {
      "package": "/path/to/truespec/.opencode/plugins/truespec"
    }
  ]
}
```

For a manually configured OpenCode server with a different PATH, set the plugin's `options.binary` to the absolute TrueSpec binary path.

## Standalone agent workflow

A standalone review does not start an arbitrary coding agent.
The Review panel includes a collapsed **Agent not polling?** prompt that can be copied into an existing agent chat.

The agent-side loop is:

```sh
# Wait up to 30 minutes for a batch or review end.
truespec listen --session "$REVIEW_ID" --idle-timeout 30m --json

# Record receipt and renew the lease.
truespec ack "$BATCH_ID" --lease "$LEASE"

# After handling the batch, mirror the final Markdown response.
truespec reply "$BATCH_ID" --body-file -
```

Use `--root /path/to/project` when the agent is not running inside the reviewed workspace.
Repeat `ack` before the five-minute lease expires if processing takes longer.
Start `listen` again after each batch and stop when the event is `ended` or `idle_timeout`.
Use `complete` only when recording partial item progress before the final batch reply.

Delivery is at least once.
If an agent edits a file but loses its lease before recording completion, the item can be delivered again.
Agents should use stable IDs and inspect current source before repeating an external action.

## Command reference

### `truespec serve`

Starts the local document viewer and review service.

```text
truespec serve [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--root <PATH>` | Select the workspace explicitly instead of discovering it. |
| `--source <openspec\|markdown>` | Choose the document-source adapter. Defaults to `openspec`. |
| `--docs <PATH>` | Select the document directory inside the workspace. |
| `--port <PORT>` | Select a fixed loopback port. `0` asks the OS to choose. |
| `--json` | Emit a versioned JSON startup record for connectors. |
| `--reset-state` | Remove only this workspace's `truespec/.state/` before starting. |

Stop other TrueSpec processes for the workspace before using `--reset-state`.
TrueSpec prints the exact state directory it removes and refuses symlinked state paths.
Markdown and `truespec/config/` remain untouched.

### `truespec setup`

Lists and installs optional coding-harness connectors.

```text
truespec setup [--connector <ID>]... [--all] [--list] [--force]
```

| Option | Meaning |
| --- | --- |
| `--connector <ID>` | Install one connector. Repeat for several. |
| `--all` | Install every supported connector. |
| `--list` | Show connectors and installation state without changing files. |
| `--force` | Replace a differing generated connector file. |

With no options in an interactive terminal, setup presents a selection list.
With no selection in a non-interactive environment, it prints the available connectors and the exact command to use.

### `truespec listen`

Listens for manual inbox work and prints one versioned delivery envelope.

```text
truespec listen --session <REVIEW_ID> [--idle-timeout <DURATION>] [--json]
```

The inactivity timeout defaults to `30m`, accepts human-readable durations, and is capped at 24 hours.
The command returns immediately when feedback arrives or the review ends.

### `truespec ack`

Records agent receipt and renews the delivery lease.

```text
truespec ack <BATCH_ID> --lease <TOKEN>
```

### `truespec complete`

Marks one handled comment complete without resolving it for the reviewer.

```text
truespec complete <BATCH_ID> <COMMENT_ID> --lease <TOKEN>
```

### `truespec reply`

Stores one final Markdown response for a manually delivered batch and marks its items handled.

```text
truespec reply <BATCH_ID> --body-file <PATH|->
```

Use `--body-file -` to read from stdin.
Retries with the same batch and body are idempotent.

### `truespec relink`

Associates a missing reviewed path with a moved Markdown file.

```text
truespec relink <OLD_PATH> <NEW_PATH>
```

The old path must be missing.
The destination must be an unclaimed Markdown file inside the workspace.
Submitted snapshots keep their historical source paths.

### `truespec connector`

Provides the versioned local protocol used by coding-harness integrations.
It is normally called by connector implementations rather than by reviewers.
See [docs/connector.md](docs/connector.md) for attach, poll, delivered, defer, leasing, retry, and receipt semantics.

Every subcommand also supports the global `--root <PATH>` option where a workspace is required.
Use `truespec <COMMAND> --help` for the authoritative flags in the installed version.

## Storage and state

Review state lives at:

```text
truespec/.state/review.sqlite3
```

Keep `truespec/.state/` out of version control.
The database stores document identities, anchors, drafts, immutable batch snapshots, leases, connector pairings, delivery receipts, and reviewer resolution state.

Markdown content is never copied into a TrueSpec-owned documentation tree.
Submitted batch snapshots preserve exactly what the agent received even when the source changes later.

## Security and privacy

- The server binds to IPv4 loopback by default.
- Mutating HTTP requests require the expected same-origin browser origin.
- Document routes expose only discovered Markdown documents.
- `truespec/.state/` and `truespec/config/` are excluded from Markdown discovery.
- Symlinks cannot escape a selected document tree.
- Raw Markdown HTML and scripts do not execute.
- External images are not loaded; image alt text remains readable.
- Safe `http`, `https`, and `mailto` links are allowed.
- State reset refuses symlinked state directories.
- The browser and core contain no model credentials or model runtime.

## Architecture

TrueSpec is one Rust binary with ordinary internal boundaries:

| Area | Responsibility |
| --- | --- |
| `source` | Document-source interface, workspace discovery, OpenSpec grouping, and Markdown-folder traversal. |
| `markdown` | Safe GFM rendering, heading fragments, rendered-text projection, and source offset maps. |
| `anchor` | Line, paragraph, list, section, selection, and general-feedback identity and relocation. |
| `review` | SQLite schema, document identities, drafts, sessions, immutable batch snapshots, and resolution. |
| `inbox` | Leasing, acknowledgement, retry, completion, and connector delivery envelopes. |
| `web` | Loopback HTTP API and embedded frontend assets. |
| `setup` | Coding-harness connector registry, discovery status, and safe installation. |

The browser assets in `web/` are embedded with `include_str!`.
There is no generated frontend directory to deploy.
Rendered document HTML stays in memory.

The connector boundary is documented in [docs/connector.md](docs/connector.md).
OpenCode is one adapter over that protocol, not a dependency of the review core.

## Current boundaries

The MVP intentionally does not provide:

- Browser-based source editing.
- Hosted accounts or cloud synchronization.
- Multi-user collaboration.
- Automatic launching of arbitrary coding agents.
- A dynamic connector marketplace.
- Exactly-once guarantees for edits performed outside TrueSpec.
- Non-Markdown document rendering.

Documents must be valid UTF-8 Markdown and no larger than 2 MiB.
TrueSpec is currently verified on macOS and Linux.

## Development

Run the ordinary quality gate:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Run the real-browser flow for both document adapters:

```sh
cargo build --locked
node tests/browser.mjs
```

The browser test requires Node.js and `chrome-devtools-axi`.
It covers desktop and mobile layouts, annotation targets, unified review state, batching, restart persistence, delivery, drawers, and agent handoff.

Run the isolated OpenCode integration:

```sh
node tests/opencode.mjs
```

That test starts an isolated OpenCode V2 server and deterministic local model fixture.
It verifies two review rounds in one real OpenCode chat and confirms that Markdown updates appear at the original TrueSpec URL.

TrueSpec itself is developed with OpenSpec.
The active MVP change is under [`openspec/changes/build-truespec-mvp`](openspec/changes/build-truespec-mvp/).

## Inspiration

TrueSpec takes inspiration from Lavish's contextual annotation and agent feedback loop.
Lavish is not a runtime dependency, and TrueSpec does not adopt Lavish's design workflow or visual design.

## License

TrueSpec is available under the [MIT License](LICENSE).
