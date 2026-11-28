# document-reading Specification

## Purpose
Let reviewers browse the Markdown their project already uses, without copying documents or changing their source workflow.

## Requirements

### Requirement: Initialized reader startup

An installed integration SHALL obtain a workspace-scoped loopback reader URL from the shared service using the initialized project's versioned source configuration.
The shared reader SHALL NOT require Node.js, OpenSpec, or the selected coding harness after the workspace context is attached.

#### Scenario: Open an initialized OpenSpec review

- **WHEN** a user invokes a selected integration in a project configured with the OpenSpec adapter
- **THEN** the shared service validates or loads that workspace context and displays its configured OpenSpec documents

#### Scenario: Open an initialized Markdown review

- **WHEN** a user invokes a selected integration in a project configured with the Markdown adapter
- **THEN** the same shared reader and annotation controls display Markdown under that workspace's configured source path

#### Scenario: Open two initialized projects

- **WHEN** integrations in two initialized repositories open reviews concurrently
- **THEN** one listener serves separate workspace-scoped catalogs and review boards
- **AND** navigation, source refresh, browser history, and old tabs remain bound to their original workspace

#### Scenario: Invoke TrueSpec before initialization

- **WHEN** an integration tries to open a review in a repository without valid TrueSpec project configuration
- **THEN** it does not register or open that workspace and tells the user to run `truespec init` from the repository root

#### Scenario: Workspace becomes unavailable

- **WHEN** an existing browser tab requests a registered workspace whose root is missing or no longer validates
- **THEN** the reader reports that workspace as unavailable
- **AND** it does not substitute another workspace or expose its documents

### Requirement: OpenSpec document groups

TrueSpec SHALL distinguish current specifications, active change artifacts, change deltas, and archived change artifacts.
It SHALL display each Markdown artifact in place without merging deltas or changing workflow state.

#### Scenario: Read a current spec and its proposed delta

- **WHEN** `openspec/specs/` and an active `openspec/changes/` directory contain specifications for the same capability
- **THEN** the reader lists them as distinct documents and renders the contents of each independently

#### Scenario: Read an unfamiliar or incomplete artifact

- **WHEN** an active change contains an unfamiliar Markdown artifact or an unfinished proposal
- **THEN** the reader lists the readable file without requiring a known schema or a complete change

#### Scenario: Group active and archived changes

- **WHEN** the workspace contains one or more active changes and archived changes
- **THEN** the sidebar places named active changes under one expanded **Changes** disclosure
- **AND** it places named archived changes under one expanded **Archives** disclosure
- **AND** the semantic roots appear in **Current specifications**, **Changes**, **Archives** order
- **AND** named active changes are open by default while named archived changes are closed by default

#### Scenario: Navigate one change and its deltas

- **WHEN** an active change contains ordinary artifacts and one or more delta specifications
- **THEN** the sidebar groups ordinary artifacts under **Changes** > **<name>**
- **AND** it groups capability names under one expanded **delta** folder without changing their source paths

### Requirement: Provide structured document navigation

Each document-source adapter SHALL provide an ordered navigation path made of stable segment IDs, display labels, and default disclosure states.
The browser SHALL render that structure without parsing separators or source-specific naming conventions from labels.

#### Scenario: Structure current OpenSpec capabilities

- **WHEN** current specifications include `delivery/flux-reconciliation` and `delivery/image-and-release`
- **THEN** the OpenSpec adapter places both documents under **Current specifications** and nested **delivery** segments
- **AND** their leaf labels are **flux-reconciliation** and **image-and-release**
- **AND** no directory label contains a trailing slash

#### Scenario: Structure nested OpenSpec artifacts

- **WHEN** a change, archive, delta, unfamiliar artifact, or custom-schema artifact contains nested directories
- **THEN** the OpenSpec adapter emits shared **Changes** or **Archives** lifecycle segments, one named change or archive segment, and one segment for every remaining semantic or real directory level
- **AND** it preserves the artifact's real source path

#### Scenario: Structure plain Markdown directories

- **WHEN** a Markdown source contains documents at arbitrary directory depths
- **THEN** the Markdown adapter emits **Documents** followed by one navigation segment for each directory
- **AND** the final document label contains only its filename

### Requirement: Collapse document navigation groups

The Documents sidebar SHALL render every navigation segment as a keyboard-accessible disclosure.
Nested folders SHALL remain independently collapsible at arbitrary depth.
The browser SHALL remember each disclosure's state locally.

#### Scenario: Open the sidebar for the first time

- **WHEN** no saved disclosure state exists
- **THEN** current specifications, active changes, and plain Markdown directory groups are open
- **AND** archived changes are closed
- **AND** nested directories and `delta` folders are open

#### Scenario: Collapse a document group

- **WHEN** a reviewer toggles any navigation segment
- **THEN** only that segment's descendant folders and document links are hidden or shown
- **AND** the document currently displayed in the reader does not change

#### Scenario: Persist independent group state

- **WHEN** a reviewer changes disclosure states at any depth and reloads the page
- **THEN** each disclosure restores its own saved state independently

#### Scenario: Reveal the active document

- **WHEN** navigation or a refreshed URL selects a document inside one or more closed folders
- **THEN** the sidebar opens every ancestor disclosure needed to reveal the active document link
- **AND** the revealed state becomes the saved state for those ancestors

#### Scenario: Use disclosures in a narrow drawer

- **WHEN** the Documents panel is shown on a narrow viewport
- **THEN** every hierarchy level retains the same disclosure, keyboard, and persistence behavior as desktop

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
