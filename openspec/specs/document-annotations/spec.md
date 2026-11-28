# document-annotations Specification

## Purpose
Preserve what a reviewer meant to comment on, even when Markdown formatting or external edits make the location harder to recover.

## Requirements

### Requirement: Contextual annotations

Reviewers SHALL be able to attach a comment to a mapped Markdown source line, paragraph, whole list, section, or exact rendered text selection.
An annotation SHALL retain its document identity, source location, revision, heading context, original quote and nearby text when applicable, and source range when reliably available.

#### Scenario: Select text across inline formatting

- **WHEN** a reviewer selects text spanning plain words, emphasis, and inline code and saves a comment
- **THEN** the annotation preserves the rendered quote in reading order without inserting Markdown delimiters
- **AND** any recorded byte range refers to the original Markdown, not browser text offsets

#### Scenario: Position the contextual selection action

- **WHEN** a reviewer selects mapped text while annotation is enabled
- **THEN** **Comment on selection** appears above the selection when space permits and below it otherwise
- **AND** it stays inside the visible reader without covering selected text
- **AND** it repositions on scrolling, resizing, and visual viewport changes

#### Scenario: Comment on a wrapped line

- **WHEN** a reviewer hovers over an authored line that wraps across several display rows
- **THEN** all rows belonging to that source line receive a border as one target
- **AND** clicking the line opens a focused comment box below it without a separate control or mode
- **AND** pressing Enter adds the comment to drafts without submitting a batch

#### Scenario: Target a paragraph from the inner margin

- **WHEN** a reviewer hovers beside a paragraph in the inner left margin
- **THEN** a single outline covers the entire paragraph, including text from multiple Markdown lines
- **AND** clicking uses the same comment composer and draft queue as line comments

#### Scenario: Target a section from the outer margin

- **WHEN** a reviewer hovers in the outer left margin of a section
- **THEN** a single outline covers the heading and its content, including nested subsections
- **AND** the section ends at the next heading of the same or a higher level, or at the end of the document

#### Scenario: Target a whole list from the inner margin

- **WHEN** a reviewer hovers in the inner left margin beside an ordered or unordered list
- **THEN** one outline covers the list and its nested items without including a label or heading above it
- **AND** clicking and pressing Enter in the composer queues a draft with the full list's source range

#### Scenario: Preview a target without a floating badge

- **WHEN** a reviewer hovers over any annotatable target
- **THEN** TrueSpec shows only the target outline without a floating line or scope label
- **AND** the opened composer still names the target for context and accessibility

#### Scenario: Cancel or type a multiline comment

- **WHEN** a reviewer presses Shift+Enter in the line comment box
- **THEN** the box inserts a newline without queuing the draft
- **AND** pressing Escape closes the box without queuing it

#### Scenario: Turn annotation off to navigate links

- **WHEN** the reviewer turns off **Annotate** in the document toolbar
- **THEN** hover outlines, the contextual selection action, and new annotation composers are disabled
- **AND** Markdown links remain navigable while existing sidebar annotations can still jump to their targets

#### Scenario: Read a previously saved section annotation

- **WHEN** a reviewer opens a workspace containing a previously saved section draft or annotation
- **THEN** they can still view that comment and send it if it is a draft

### Requirement: Persistent cross-document review feed

The review sidebar SHALL show one chronological feed without separate Drafts and History views.
Each item SHALL show its document path and target quote or original heading, with the source line number for line comments.
Reviewers SHALL be able to jump to the target and edit or remove unsent drafts.
Submitting SHALL update an item in place with its delivery and source-freshness state rather than moving it elsewhere.

#### Scenario: Review drafts across documents after restart

- **WHEN** a reviewer saves drafts on different documents and restarts TrueSpec
- **THEN** the feed still shows both drafts in their original order with each document path
- **AND** navigating to one draft does not discard the other

#### Scenario: Send without reordering comments

- **WHEN** the reviewer sends several drafts and later adds another draft
- **THEN** the submitted items retain their feed positions with updated status
- **AND** the new draft appears at the end of the same feed

#### Scenario: Expand long context in the review feed

- **WHEN** expanded original context makes the chronological feed taller than the Review panel
- **THEN** the feed scrolls independently
- **AND** the general feedback box and send controls remain reachable without growing the whole panel

### Requirement: Show submitted feedback status

Submitted feedback SHALL show agent progress and source-freshness warnings.
The browser SHALL NOT expose reviewer resolution controls or resolution badges.

#### Scenario: Read submitted feedback status

- **WHEN** feedback has been submitted
- **THEN** the feed shows agent progress and any source-freshness warning
- **AND** it does not show **Resolve**, **Reopen**, **Resolved**, or **Unresolved**

### Requirement: Queue feedback without an annotation

An open review SHALL provide a collapsible general-feedback composer directly above its send controls.
The disclosure control SHALL expose its state and remain operable with pointer, keyboard, and touch input.
Collapsing SHALL preserve pending text and submission availability.
Non-empty text SHALL be included by **Send** or **Send and End** even while the composer is collapsed.
Feedback submitted from this composer SHALL have no document path, source range, or annotation target.

#### Scenario: Queue general feedback

- **WHEN** the reviewer types general feedback and presses Enter
- **THEN** the text clears and a dashed draft is appended to the chronological feed without a Draft badge
- **AND** the status message reads **Comment queued.**

#### Scenario: Write multiple lines or recover from failure

- **WHEN** the reviewer presses Shift+Enter in the general feedback box
- **THEN** a newline is inserted without queuing the text
- **AND** if queuing or submission fails, the typed text or its resulting draft remains available to retry

#### Scenario: Send typed general feedback directly

- **WHEN** the general feedback box contains non-empty text and the reviewer chooses **Send**
- **THEN** the text is submitted in the same batch as any queued drafts without requiring Enter first
- **AND** the text box clears only after submission succeeds
- **AND** **Send** is enabled while that text is available

#### Scenario: Reveal the composer disclosure control

- **WHEN** a pointer hovers the composer separator, keyboard focus reaches its control, or a touch layout displays the composer
- **THEN** a labeled collapse or expand icon button is visible
- **AND** the control exposes the current expanded state and the content it controls

#### Scenario: Collapse pending general feedback

- **WHEN** the reviewer collapses a composer containing non-empty text
- **THEN** the textarea and hint are hidden while the text remains available to **Send** and **Send and End**
- **AND** the compact control indicates pending feedback visually and through its accessible label
- **AND** expanding restores the same text and focuses the textarea

#### Scenario: Restore the composer preference

- **WHEN** the reviewer reloads after changing the composer disclosure state
- **THEN** the browser restores that local preference
- **AND** the remaining disclosure control can always restore the composer

### Requirement: Adapter-independent document identity

The same workspace file exposed by different adapters SHALL retain its annotation history.
Missing paths SHALL be shown as missing rather than reassigned to a similarly named or moved file.
Submitted snapshots SHALL retain their historical document identity and source evidence.

#### Scenario: Switch document adapters

- **WHEN** a reviewer opens the same source file through a different adapter in the same workspace
- **THEN** the file shows its existing drafts and submitted annotations

#### Scenario: Move a reviewed file

- **WHEN** a reviewed file is moved within the workspace
- **THEN** its history still shows the original location and identifies the source as missing
- **AND** TrueSpec does not attach that history to the new path automatically

### Requirement: Conservative re-anchoring

TrueSpec SHALL relocate an anchor after a source edit only when it finds a reliable unique match.
Otherwise it SHALL mark the anchor as needing review or missing and retain its original evidence.

#### Scenario: Shift an unchanged quote

- **WHEN** lines are inserted above an annotation but the quote, its heading, and its identifying context still match uniquely
- **THEN** jumping to the annotation lands on the quote in the latest revision

#### Scenario: Duplicate the identifying context

- **WHEN** an edit leaves two locations with the same quote and indistinguishable identifying context
- **THEN** the annotation shows **Outdated** instead of choosing a location
- **AND** the reviewer can still read the original quote and comment
