# Document Reading

## Purpose

Let reviewers browse the Markdown their project already uses, without copying documents or changing their source workflow.

## ADDED Requirements

### Requirement: Local reader startup

`truespec serve` SHALL start a loopback-only reader without requiring a TrueSpec configuration file, Node.js, OpenSpec, or Lavish at runtime.
It SHALL accept an explicit workspace root and a document source selection.

#### Scenario: Discover OpenSpec from a nested directory

- **WHEN** the reviewer runs `truespec serve` from a subdirectory of a workspace containing `openspec/`
- **THEN** the command prints a local browser URL and displays the workspace's OpenSpec documents

#### Scenario: Review a plain Markdown folder

- **WHEN** the reviewer starts the viewer with `--source markdown` and points it at a folder of Markdown files without OpenSpec
- **THEN** the same document reader and annotation controls are available

#### Scenario: Reset local review state for testing

- **WHEN** the reviewer starts the viewer with `--reset-state`
- **THEN** TrueSpec prints and removes only that workspace's real `truespec/.state/` directory before creating a fresh database
- **AND** source Markdown and `truespec/config/` remain unchanged

### Requirement: OpenSpec document groups

TrueSpec SHALL distinguish current specifications, active change artifacts, change deltas, and archived change artifacts.
It SHALL display each Markdown artifact in place without merging deltas or changing workflow state.

#### Scenario: Read a current spec and its proposed delta

- **WHEN** `openspec/specs/` and an active `openspec/changes/` directory contain specifications for the same capability
- **THEN** the reader lists them as distinct documents and renders the contents of each independently

#### Scenario: Read an unfamiliar or incomplete artifact

- **WHEN** an active change contains an unfamiliar Markdown artifact or an unfinished proposal
- **THEN** the reader lists the readable file without requiring a known schema or a complete change

#### Scenario: Navigate one change and its deltas

- **WHEN** an active change contains ordinary artifacts and one or more delta specifications
- **THEN** the sidebar groups ordinary artifacts under a lowercase `change / <name>` heading
- **AND** it groups capability names under one expanded `delta/` folder without changing their source paths

### Requirement: Shared Markdown rendering and refresh

The reader SHALL render headings, lists, tables, fenced code, and links through the same pipeline for both sources.
It SHALL refresh externally edited documents without losing the text of an open comment composer.

#### Scenario: Follow document links and heading anchors

- **WHEN** a reviewer follows a relative link from one discovered document to a heading in another
- **THEN** the reader opens that document and navigates to the heading

#### Scenario: Edit the source while composing

- **WHEN** an external editor saves the active document while the reviewer is writing a comment
- **THEN** TrueSpec preserves the comment text and makes the updated document available without silently applying the old selection to the new revision

### Requirement: Restrict document access

TrueSpec SHALL serve only discovered source documents through its document routes.
It SHALL exclude configuration and local review state and SHALL NOT execute active content embedded in Markdown.

#### Scenario: Hidden state and unsafe Markdown

- **WHEN** a broad Markdown source contains the TrueSpec state directory, a link outside the source, or Markdown containing raw scripts
- **THEN** the reader does not expose the state directory or outside files, and the scripts do not execute

### Requirement: Responsive review workspace

The viewer SHALL let reviewers independently collapse Documents and Review on desktop.
On smaller viewports, each panel SHALL open as an overlay drawer without shrinking or stacking below the reader.
Panel controls SHALL remain attached to their panel headers, with compact reader-edge tabs available while collapsed.

#### Scenario: Collapse desktop panels

- **WHEN** a reviewer collapses either desktop panel
- **THEN** the reader expands into the released space and hidden panel controls cannot receive focus
- **AND** the browser remembers that panel preference

#### Scenario: Use a panel on a narrow viewport

- **WHEN** a reviewer opens Documents or Review on a narrow viewport
- **THEN** it overlays the reader, closes the other drawer, and can be dismissed by its toggle, the backdrop, or Escape
- **AND** focus returns to the opening toggle after light dismissal
