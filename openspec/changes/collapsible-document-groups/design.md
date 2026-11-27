# Design

## Context

The current `Document` response exposes `group`, `label`, and `kind` strings.
The first implementation made top-level groups collapsible and treated `delta` as one browser-side special case.
That shape cannot represent arbitrary nested capability paths without parsing slash-delimited display labels in JavaScript.
The catalog is rebuilt after source refreshes and document changes, while document links preserve focus through their stable document IDs.

## Goals / Non-Goals

**Goals:**

- Make source adapters authoritative for navigation hierarchy and labels.
- Render arbitrary navigation depth without browser knowledge of OpenSpec or filesystem conventions.
- Persist every disclosure state independently.
- Always reveal the selected document.
- Use native keyboard and assistive-technology semantics.
- Keep behavior identical in desktop sidebars and mobile drawers.

**Non-Goals:**

- Add expand-all or collapse-all controls.
- Change source paths, document identity, or document discovery.
- Synchronize disclosure preferences across browsers or repositories.

## Decisions

### Add structured navigation metadata to documents

Replace browser-facing structural strings with ordered navigation segments:

```rust
struct NavigationSegment {
    id: String,
    label: String,
    default_open: bool,
}

struct Document {
    path: String,
    label: String,
    navigation: Vec<NavigationSegment>,
    kind: &'static str,
}
```

Segment IDs are adapter-owned stable identities.
Labels are presentation text and never encode hierarchy with `/`.
The browser uses IDs for persistence and labels only for display.

### Derive OpenSpec hierarchy in OpenSpecSource

The OpenSpec adapter maps discovered artifacts into semantic roots and real nested paths:

- `specs/delivery/flux-reconciliation/spec.md` becomes **Current specifications** > **delivery** > **flux-reconciliation**.
- `changes/name/specs/delivery/flux-reconciliation/spec.md` becomes **change / name** > **delta** > **delivery** > **flux-reconciliation**.
- Archived artifacts use **archive / name** as the root, closed by default.
- Ordinary, unfamiliar, and custom-schema artifacts retain every real directory level beneath their change or archive root.

Only the adapter understands those boundaries.
The browser does not inspect OpenSpec paths or kinds to reconstruct folders.

### Derive Markdown hierarchy in MarkdownFolderSource

The Markdown adapter emits a **Documents** root followed by each directory segment relative to its configured source.
The document label is the filename only.
This produces the same tree model for root files and arbitrary nested directories.

### Render a generic recursive tree

Build a tree from segment IDs, then render every tree node as native `details` and `summary`.
Native elements provide Enter and Space activation, announced expanded state, and focus behavior without custom ARIA state management.

Hide the browser's default marker and use one restrained chevron that rotates with the `open` state.
Documents remain links at the leaf positions supplied by adapters.

### Persist state by stable group identity

Store one boolean per segment ID in local storage.
Full adapter-owned IDs distinguish identically named folders under different ancestors.
Missing or invalid storage falls back to defaults without blocking navigation.

### Use source-aware defaults

Default open state is:

- `Current specifications`: open.
- `change / ...`: open.
- `archive / ...`: closed.
- Other Markdown directory groups: open.
- Nested directories and `delta`: open.

This keeps current work immediately available while reducing archive noise.

### Reveal selected documents during rendering

Before replacing the catalog DOM, follow the active document's supplied navigation path.
Force every matching segment open and save each open state.

This applies when:

- A document link is followed.
- Browser history changes the selected document.
- The page loads with a `doc` query parameter.
- A catalog refresh rebuilds the sidebar.

Programmatic reveal intentionally overrides a previously closed state because the active link must remain visible.

### Preserve state through catalog refreshes

Attach `toggle` listeners when creating each disclosure.
Ignore toggle events caused by initial property assignment until the element is connected, then persist user and programmatic changes.

Continue restoring focus by document ID after replacing the catalog.
If focus was on a folder summary, restore it by segment ID.

## Risks / Trade-offs

- **Adapter hierarchy changes after source changes** -> Treat new segment IDs as new folders and tolerate stale local-storage entries.
- **The selected document is inside a group the reviewer closed intentionally** -> Opening a selected document takes precedence so navigation never hides its destination.
- **Catalog refresh toggle events overwrite saved state** -> Set initial state before insertion and attach persistence listeners deliberately.
- **Many archived groups create many local-storage entries** -> Store only small booleans and tolerate stale keys.
- **Custom chevrons can duplicate native markers** -> Hide native markers consistently and verify keyboard semantics in the accessibility tree.
- **Two adapters emit inconsistent IDs** -> Keep IDs namespaced by adapter and cover representative OpenSpec and Markdown trees in Rust tests.

## Migration Plan

No server or data migration is required.
Existing label-based disclosure preferences are not migrated because the structured IDs have different semantics.
Users receive source-aware defaults once, then new segment-ID preferences persist after toggles.
