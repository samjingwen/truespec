# Design

## Context

`renderNav` currently renders each source group as a plain `div` with an `h2`.
OpenSpec delta documents already use one native `details` element inside their parent group, but that nested folder is always opened on every render.
The catalog is rebuilt after source refreshes and document changes, while document links preserve focus through their stable document IDs.

## Goals / Non-Goals

**Goals:**

- Collapse every top-level group without changing source grouping or document ordering.
- Persist top-level and nested disclosure state independently.
- Always reveal the selected document.
- Use native keyboard and assistive-technology semantics.
- Keep behavior identical in desktop sidebars and mobile drawers.

**Non-Goals:**

- Add a recursive file-system tree beyond the existing top-level and `delta/` hierarchy.
- Add expand-all or collapse-all controls.
- Change document labels, sorting, grouping, or source adapter output.
- Synchronize disclosure preferences across browsers or repositories.

## Decisions

### Use native details and summary elements

Render each `doc-group` as `details` with a `summary` containing the existing group label.
Keep nested `delta-group` as its own `details`.
Native elements provide Enter and Space activation, announced expanded state, and focus behavior without custom ARIA state management.

Hide the browser's default marker and use one restrained chevron that rotates with the `open` state.
Group contents remain flat links beneath the summary, preserving the existing visual hierarchy.

### Persist state by stable group identity

Store one boolean per disclosure in local storage.
Keys include:

- The top-level group label.
- The parent group label plus `delta/` for nested folders.

Group labels are the stable identities already produced by source adapters, such as `Current specifications`, `change / name`, `archive / name`, and Markdown directory paths.
Missing or invalid storage falls back to defaults without blocking navigation.

### Use source-aware defaults

Default open state is:

- `Current specifications`: open.
- `change / ...`: open.
- `archive / ...`: closed.
- Other Markdown directory groups: open.
- Nested `delta/`: open.

This keeps current work immediately available while reducing archive noise.

### Reveal selected documents during rendering

Before replacing the catalog DOM, identify the group and nested folder containing `currentId`.
Force those disclosures open and save their open state.

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
If focus was on a group summary, restore it by the disclosure's stable key.

## Risks / Trade-offs

- **Group labels change after source changes** -> Treat the renamed group as new and use its default state; stale local-storage entries are harmless.
- **The selected document is inside a group the reviewer closed intentionally** -> Opening a selected document takes precedence so navigation never hides its destination.
- **Catalog refresh toggle events overwrite saved state** -> Set initial state before insertion and attach persistence listeners deliberately.
- **Many archived groups create many local-storage entries** -> Store only small booleans and tolerate stale keys.
- **Custom chevrons can duplicate native markers** -> Hide native markers consistently and verify keyboard semantics in the accessibility tree.

## Migration Plan

No server or data migration is required.
Existing users receive source-aware defaults on first load.
Their disclosure choices are stored after the first toggle.
