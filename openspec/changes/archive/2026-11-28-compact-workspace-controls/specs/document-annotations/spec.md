# Document Annotations Delta

## MODIFIED Requirements

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
