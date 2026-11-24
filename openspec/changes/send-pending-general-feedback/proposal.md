# Proposal

## Why

Typing in the persistent general feedback box and clicking **Send** or **Send and End** currently ignores that unqueued text.
The review can end successfully while silently dropping the reviewer's final message, as happened with the feedback `"apply"`.

## What Changes

- Treat non-empty general feedback text as part of the send action even when the reviewer does not press Enter first.
- Include that feedback in the same durable batch and, for **Send and End**, the same review-ending transaction.
- Enable **Send** when either queued drafts or non-empty general feedback are available.
- Clear the general feedback box only after submission succeeds.
- Preserve the text or resulting draft and keep the review open when queuing, confirmation, or submission fails.
- Add browser and store regression coverage for direct send, direct send-and-end, idempotent retries, and failure recovery.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-annotations`: Let send actions capture typed general feedback without requiring a separate Enter step.
- `review-handoff`: Include pending general feedback in durable submission and prevent **Send and End** from discarding it.

## Impact

- Affects the submission payload, SQLite submission transaction, general feedback controls, send-button state, and browser and Rust tests.
- Does not change the database schema, connector protocol, source adapters, or annotation anchors.
