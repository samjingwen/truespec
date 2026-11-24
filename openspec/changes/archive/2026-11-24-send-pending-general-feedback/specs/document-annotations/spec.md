# Document Annotations Delta

## MODIFIED Requirements

### Requirement: Queue feedback without an annotation

An open review SHALL show a persistent text box directly above its send controls.
Feedback queued from this box SHALL have no document path, source range, or annotation target.
Non-empty text still present in the box SHALL be included when the reviewer chooses **Send** or **Send and End**.

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
