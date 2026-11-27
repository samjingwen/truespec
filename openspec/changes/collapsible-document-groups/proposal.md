# Proposal

## Why

The Documents sidebar grows long as current specifications, active changes, archives, and Markdown directories accumulate.
Collapsible groups let reviewers focus on relevant work while keeping the active document visible and preserving their preferred sidebar organization.

## What Changes

- Extend the document-source contract with ordered navigation segments containing stable IDs, display labels, and default disclosure state.
- Derive OpenSpec hierarchy inside `OpenSpecSource` from discovered schema and artifact paths.
- Derive plain Markdown hierarchy inside `MarkdownFolderSource` from actual directories.
- Render every navigation level as a native collapsible disclosure without parsing display labels in the browser.
- Keep current specifications, active changes, and plain Markdown directories open by default.
- Keep archived changes closed by default.
- Preserve each group's open state locally across reloads.
- Preserve every nested folder state independently from its ancestors.
- Automatically open all ancestor disclosures when navigation selects a document inside a collapsed group.
- Display directory names such as `delivery` and `delta` without structural slash characters.
- Use the same keyboard-accessible behavior on desktop and narrow drawer layouts.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-reading`: Add persistent, nested, accessible disclosure behavior to document navigation groups.

## Impact

- Affects the document-source API model, OpenSpec and Markdown adapters, browser navigation rendering, local-storage preferences, sidebar styling, refresh behavior, and Rust and browser tests.
- Does not change document discovery, source paths, current-document identity, review state, or server APIs.
