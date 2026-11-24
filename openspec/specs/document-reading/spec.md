# document-reading Specification

## Purpose
Let reviewers browse the Markdown their project already uses, without copying documents or changing their source workflow.

## Requirements

### Requirement: Initialized reader startup

An installed integration SHALL start the loopback reader from the initialized project's versioned source configuration.
The reader SHALL NOT require Node.js, OpenSpec, or the selected coding harness after the integration starts it.

#### Scenario: Open an initialized OpenSpec review

- **WHEN** a user invokes a selected integration in a project configured with the OpenSpec adapter
- **THEN** TrueSpec starts or reuses the loopback reader and displays that source path's OpenSpec documents

#### Scenario: Open an initialized Markdown review

- **WHEN** a user invokes a selected integration in a project configured with the Markdown adapter
- **THEN** the same reader and annotation controls display Markdown under the configured source path

#### Scenario: Invoke TrueSpec before initialization

- **WHEN** an integration tries to start a review in a repository without valid TrueSpec project configuration
- **THEN** it does not start the reader and tells the user to run `truespec init` from the repository root

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

### Requirement: Use consistent interface motion

TrueSpec SHALL use its normal interface transitions and progress animations regardless of the operating system's reduced-motion preference.

#### Scenario: Open TrueSpec with reduced motion enabled

- **WHEN** the browser reports `prefers-reduced-motion: reduce`
- **THEN** drawer transitions, switch transitions, resize feedback, and progress spinners use the same motion as the default preference

#### Scenario: Show agent working with reduced motion enabled

- **WHEN** submitted feedback is waiting for the agent and the browser reports reduced motion
- **THEN** the working spinner continues rotating

### Requirement: Restrict document access

TrueSpec SHALL serve only discovered source documents through its document routes.
It SHALL exclude configuration and local review state and SHALL NOT execute active content embedded in Markdown.

#### Scenario: Hidden state and unsafe Markdown

- **WHEN** a broad Markdown source contains the TrueSpec state directory, a link outside the source, or Markdown containing raw scripts
- **THEN** the reader does not expose the state directory or outside files, and the scripts do not execute

### Requirement: Responsive review workspace

The viewer SHALL let reviewers independently collapse Documents and Review on desktop.
It SHALL let reviewers temporarily expand Review across the complete workspace and adjust the open Review width on desktop.
On smaller viewports, each panel SHALL remain an overlay drawer.
Panel controls SHALL remain keyboard accessible and expose their current state.

#### Scenario: Collapse desktop panels

- **WHEN** a reviewer collapses either desktop panel
- **THEN** the reader expands into the released space and hidden panel controls cannot receive focus
- **AND** the browser remembers that panel preference

#### Scenario: Use a panel on a narrow viewport

- **WHEN** a reviewer opens Documents or Review on a narrow viewport
- **THEN** it overlays the reader, closes the other drawer, and can be dismissed by its toggle, the backdrop, or Escape
- **AND** focus returns to the opening toggle after light dismissal

#### Scenario: Expand and restore Review

- **WHEN** a reviewer activates the expand control immediately before the **Review** heading
- **THEN** Review fills the complete workspace below the app header while Documents and the reader are hidden and inert
- **AND** the control identifies the expanded state and changes its accessible action to **Restore Review**
- **AND** activating it again or pressing Escape restores the exact prior panel layout and width
- **AND** expanded state is not persisted across reloads

#### Scenario: Resize Review with a pointer

- **WHEN** a reviewer drags the separator on Review's left edge on a desktop viewport
- **THEN** Review follows the pointer between defined minimum and maximum widths without making the document reader unusable
- **AND** the chosen width is stored for later page loads
- **AND** text selection and layout transitions do not interfere with the drag

#### Scenario: Resize Review with a keyboard

- **WHEN** the desktop separator has keyboard focus
- **THEN** Left Arrow widens Review, Right Arrow narrows it, and Home restores the default width
- **AND** the separator exposes its orientation, current value, minimum, and maximum to assistive technology

#### Scenario: Adapt a saved width

- **WHEN** a saved Review width no longer fits after the viewport changes
- **THEN** the viewer clamps it to the current safe range
- **AND** the resize separator is unavailable while Review is collapsed, expanded, or shown as a narrow-viewport drawer
- **AND** expanding Review on a narrow viewport fills the available workspace
