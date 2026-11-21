# Design

## Context

The repository starts with a minimal Rust binary crate and no existing TrueSpec data or compatibility obligations.
The motivation is in [proposal.md](proposal.md); the [spec deltas](specs/) define the observable behavior.
The review ends in the agent's existing chat, so sending feedback and ending the browser session are related actions, but neither depends on a running agent.

## Goals / Non-Goals

**Goals:**

- Distribute one Rust executable with all browser assets embedded.
- Keep document discovery and agent delivery as independent adapter boundaries around a shared review core.
- Make submitted evidence immutable and delivery recoverable after server or agent interruption.

**Non-Goals:**

- Browser-based chat or document editing, a hosted backend, or an agent launcher.
- Framework integration, a dynamic plugin system, or a document format other than Markdown for the first release.
- Exactly-once effects for edits or messages an agent performs outside TrueSpec.

## Decisions

### Use Axum directly for the local reader

One Rust crate will keep the CLI, HTTP API, document adapters, Markdown rendering, review core, and inbox implementation in separate modules.
Clap parses CLI arguments; Axum serves compile-time embedded HTML, CSS, and JavaScript using Tokio.
Bind only to `127.0.0.1`, print the actual port and review ID, and allow an explicit port when needed.
The browser speaks same-origin HTTP only; validate the listener authority and reject cross-origin mutations.
Discard active Markdown markup, restrict generated links to safe schemes, and serve source documents only by discovered IDs.
A separate frontend server or web framework would add deployment or build coupling without helping the review flow.

### Separate document locators from adapter presentation

`DocumentSource` lists and reads documents; it exposes generic source locators, Markdown bytes, and optional display metadata.
An OpenSpec adapter classifies files by their paths without interpreting or modifying schemas.
The Markdown-folder adapter discovers `.md` files using the same safe traversal policy.
Sources operate under a canonical workspace root; exclude state, config, and paths outside the selected source boundary.
`--root` selects the workspace and `--docs` can select a location inside it.
Neither document titles nor adapter-specific classifications participate in identity.

The review store assigns stable document IDs to normalized workspace-relative paths and hashes source bytes for revisions.
An explicit relink to an unclaimed source updates the registry without altering stored locators in snapshots.
File hashes alone cannot distinguish identical files, and inferred rename matching could move a review to the wrong source.

### Build a single render and anchor model

Use the Rust `pulldown-cmark` crate with source offsets and GitHub Flavored Markdown extensions for tables, strikethrough, and task lists.
The shared renderer builds safe HTML, stable heading fragments for navigation, and a run map for selectable text.
Translate the browser's selection endpoints through that run map, validating the revision and UTF-16 offsets on the server.
Build a separate line index over the same runs so new line targets do not change run IDs used by existing text selections.
Collect paragraph and outer list spans from the Markdown parser and derive section bounds from heading levels.
An outer list span includes nested items but excludes the adjacent label or heading.
The browser uses text rectangles for line targets and block and range bounds for the two left margin targets.
The same focused composer opens without inserting anything into the Markdown.
When parts of a line cannot be mapped unambiguously, offer exact text selection instead of guessing.
Record exact source byte ranges only where endpoint mapping is reliable; for example, decoded entities need mapping rather than simple offset addition.
Preserve the quote and nearby text independently of the range.
On later revisions match a line's quote and context within the heading lineage instead of trusting its line number.
Paragraphs relocate using their complete rendered quote and neighboring evidence; sections use their existing heading lineage and context.
Do not use the closest byte offset or the first matching slug as a tiebreaker.

Periodically refresh the document index, open document revision, and review state through conditional reads while the tab is visible.
Keep an open composer intact when refreshed source invalidates its target.
Polling is simpler than a platform-specific watcher for the first release and also observes writes made through the CLI.

### Keep review sessions and handoff state in SQLite

Store local state in `truespec/.state/review.sqlite3`, separate from source Markdown.
Use schema migrations and transactions shared by the server and inbox CLI.
Use the workspace for durable draft ownership and a review ID for each browser session.
On first use create a review; after ending, open a new review only on an explicit user action.
An ended tab remains read-only, including when a new review starts elsewhere.

A submission freezes the selected draft messages and their real source context into batch items.
In the same transaction, remove the submitted drafts from the draft queue and, for **Send and End**, record the end marker.
An idempotency key identifies a browser submission across HTTP retries.
The browser offers **Send** without ending and **Send and End** for both sending and ending; with no drafts the latter only ends the review.
A transaction is a better fit than a log of independent append operations because draft submission and session closure must commit together.

### Make inbox delivery explicit and independent of the server

Use a built-in local inbox adapter over the same SQLite store, with a versioned JSON envelope and stable review, batch, and comment IDs.
Only this adapter needs to know the CLI delivery protocol.

| Agent command | Purpose |
| --- | --- |
| `truespec poll --session SESSION_ID --json --wait 30` | Lease available items; return the work and session status, or an idle or ended result. |
| `truespec ack BATCH_ID --lease TOKEN` | Record receipt and renew the current lease; repeat if processing takes longer. |
| `truespec complete BATCH_ID COMMENT_ID --lease TOKEN` | Record item completion without posting a browser message. |

Show the review ID in the viewer as well as in the startup output so a new review does not require a server restart.
An agent can supply the same workspace with `--root` when invoked elsewhere.
A single claim transaction fences competing pollers with an expiring lease token.
An agent interrupted after recording some completions will only receive outstanding items on a subsequent attempt.
An exact repeat of a completed item is a no-op; a stale token cannot complete an outstanding item.
When no more work remains for an ended review, return an ended result instead of waiting for new messages.
The agent reads the finalized feedback, returns to the original chat to discuss it, and records handled items separately from user resolution.
The UI never tries to bring a terminal to the foreground or change an agent's tool state.

### Pair coding chats through a versioned connector

The local connector protocol exposes attach, poll, delivered, and defer operations over the same review database.
One review pairs with one opaque destination, which is captured on each newly submitted batch.
Existing manual batches remain separate, and a paired delivery receipt cannot be recorded as a manual acknowledgement.
Connector claims are ordered and leased; an expired or deferred attempt can retry the same batch without changing its snapshot.

The optional OpenCode plugin owns the `/truespec-review` command and obtains the actual chat ID and working location from OpenCode.
`truespec setup` reads a built-in connector registry and interactively lists supported harnesses and installation state.
Scripts select repeatable connectors with `--connector`, or use `--all` and `--list`.
The first registry entry installs a generated OpenCode plugin under its global discovery directory and records the current binary path for service processes with a different PATH.
Each connector installer is idempotent and refuses to overwrite a differing file without `--force`.
It requests the default OpenSpec source without a file argument and reuses its viewer while that workspace is open.
It submits a normal queued message with a stable OpenCode message ID, preserving the user's current chat and its normal permissions.
TrueSpec refreshes changed Markdown independently of OpenCode.
A delivery receipt means the chat accepted the turn, not that the agent completed it.
Ending the browser review allows any queued final batch to drain and does not stop the coding chat.

## Risks / Trade-offs

- **Edited or repeated text can defeat relocation.** Prefer **Needs review** over attaching to the wrong occurrence, and preserve the original evidence for a user-confirmed submission.
- **A crash can occur after the agent made an edit but before recording completion.** Deliver at least once and include stable IDs; agents should inspect current source before repeating an external action.
- **A local HTTP listener still processes untrusted Markdown.** Restrict the file tree, disable active markup, and protect local mutations from unrelated browser origins.

## Migration Plan

Install the binary independently of OpenSpec and create the database on first use, without touching existing project documents.
The connector migration preserves old reviews and leaves existing batches in the manual inbox.
