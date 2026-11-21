# Proposal

## Why

Plain chat makes it hard to review several sections across several documents without losing what each comment refers to.
TrueSpec lets a reviewer collect contextual feedback in the browser and return to the same coding agent chat with one batch.

## What Changes

- Ship `truespec serve` as a local Rust CLI with an embedded browser UI that reads OpenSpec and Markdown-folder documents in place.
- Add direct line, paragraph, whole-list, section, text-selection, and unanchored feedback with one chronological, persistent review feed.
- Add **Send** to keep reviewing and **Send and End** to submit any drafts and close the browser review.
- Deliver immutable feedback snapshots through a durable local agent inbox, with explicit receipt, retry, and completion.
- Add a versioned connector interface and an optional OpenCode plugin that pairs `/truespec-review` with the invoking chat.
- Refresh documents after edits, re-anchor only when the match is reliable, and retain the original context otherwise.
- Keep document sources and agent connections independent of the review core and each other.

## Capabilities

### New Capabilities

- `document-reading`: Discover, render, and refresh Markdown from independent document sources without editing the source files.
- `document-annotations`: Capture source-aware annotations, keep comment and delivery state, and report anchors that need review.
- `review-handoff`: Submit durable batches, end a browser review without losing work, and hand feedback back to the agent's chat.
- `connector-protocol`: Pair one coding conversation, deliver immutable batches through a versioned leased protocol, and mirror one durable reply per batch.

### Modified Capabilities

None.

## Impact

The new binary serves a loopback HTTP UI and stores review state outside source documents.
OpenSpec is a supported document source and a development workflow for this repository, not a runtime dependency.
Browser chat, hosted services, changes to OpenSpec workflow state, and automatic agent launching are out of scope.
