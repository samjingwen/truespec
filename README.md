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
Local SQLite inbox
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
It reads `truespec/config/project.json` and opens or resumes one review paired with that exact OpenCode chat and worktree.

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

## Command reference

### `truespec init`

Initializes the current repository and installs coding integrations.

```text
truespec init [--source <ID>] [--source-path <PATH>]
              [--integration <ID>]... [--all] [--force]
```

| Option | Meaning |
| --- | --- |
| `--source <ID>` | Select a registered source adapter. Current values are `openspec` and `markdown`. |
| `--source-path <PATH>` | Select the adapter root relative to the repository. |
| `--integration <ID>` | Install one coding integration. Repeat to select several. |
| `--all` | Install every registered integration. |
| `--force` | Replace differing generated files for selected integrations. |

With no integration flags in an interactive terminal, initialization presents a multi-select.
Non-interactive initialization requires `--integration` or `--all`.
Run `truespec --help` for the authoritative public command surface.

Connector transport and viewer operations are hidden implementation details.
Their accepted behavior is defined by the [connector protocol specification](openspec/specs/connector-protocol/spec.md).

### Upgrade from the pre-initialization MVP

Run `truespec init` once from each repository root after upgrading.
The existing SQLite database and review history remain in place.
The first selected integration adopts outstanding connector-free batches without changing their snapshots or completed items.
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
| `source` | Source-adapter registry, document-source interface, OpenSpec grouping, and Markdown-folder traversal. |
| `project` | Versioned project configuration, source validation, and repository-root checks. |
| `initialize` | Multi-integration selection, complete preflight, and atomic generated-file installation. |
| `integration` | Coding-harness adapter registry and generated plugin and skill artifacts. |
| `markdown` | Safe GFM rendering, heading fragments, rendered-text projection, and source offset maps. |
| `anchor` | Line, paragraph, list, section, selection, and general-feedback identity and relocation. |
| `review` | SQLite schema, document identities, drafts, sessions, immutable batch snapshots, and resolution. |
| `inbox` | Connector leasing, receipts, retries, and delivery envelopes. |
| `web` | Loopback HTTP API and embedded frontend assets. |
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
It covers desktop and mobile layouts, annotation targets, unified review state, batching, restart persistence, delivery, drawers, and agent handoff.

Run the isolated OpenCode integration:

```sh
node tests/opencode.mjs
```

That test starts an isolated OpenCode V2 server and deterministic local model fixture.
It verifies two review rounds in one real OpenCode chat and confirms that Markdown updates appear at the original TrueSpec URL.

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
