# Proposal

## Why

The fixed Review sidebar is too narrow for detailed feedback and mirrored agent responses, while readers sometimes need to focus entirely on review history.
An explicit focus mode and adjustable desktop width let reviewers choose the right balance without disrupting the existing document workflow.

## What Changes

- Add a maximize and restore control immediately before the **Review** heading.
- Let expanded Review occupy the complete workspace below the app header while making Documents and the reader unavailable to focus and assistive technology.
- Restore the exact prior panel layout when the reviewer clicks the control again or presses Escape.
- Add a draggable desktop separator on Review's left edge with safe width limits.
- Support keyboard resizing and a default-width reset through the separator.
- Persist the adjusted Review width across reloads, but keep expanded mode temporary.
- Preserve the existing narrow-viewport drawer behavior while allowing Review focus mode to fill the mobile workspace.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-reading`: Extend the responsive review workspace with temporary full-workspace Review focus and persistent, accessible desktop resizing.

## Impact

- Affects the embedded browser markup, panel state, layout CSS, local-storage preferences, pointer and keyboard interactions, and desktop and phone browser tests.
- Does not change review persistence, HTTP APIs, source adapters, connector behavior, or Rust dependencies.
