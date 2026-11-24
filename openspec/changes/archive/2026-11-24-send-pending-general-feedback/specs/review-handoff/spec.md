# Review Handoff Delta

## MODIFIED Requirements

### Requirement: End the browser review

The browser SHALL offer **Send** to submit feedback without ending and **Send and End** to submit pending feedback and end in one action.
Non-empty text in the general feedback box SHALL count as pending feedback.
With no drafts and no pending general text, **Send** SHALL be disabled and **Send and End** SHALL end the review without a batch.
Ending SHALL NOT terminate the agent or silently discard feedback.

#### Scenario: Send and end succeeds

- **WHEN** the reviewer selects **Send and End**
- **THEN** all queued drafts, pending general feedback, and the end marker are committed together
- **AND** the browser displays **Return to your agent**
- **AND** the agent can retrieve pending work and learn the browser session ended

#### Scenario: Send and end fails

- **WHEN** pending feedback cannot be saved during **Send and End**
- **THEN** the session remains open
- **AND** the typed general feedback or resulting draft remains available to retry

#### Scenario: End an empty review

- **WHEN** the reviewer chooses **Send and End** with no queued drafts and no pending general feedback text
- **THEN** the session ends without creating a batch

#### Scenario: Send and end typed general feedback

- **WHEN** the reviewer types general feedback and chooses **Send and End** without pressing Enter
- **THEN** TrueSpec creates one general feedback item, submits it in the final batch, and ends the review
- **AND** retrying the same submission does not duplicate the feedback item or batch
