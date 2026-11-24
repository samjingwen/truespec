# Design

## Context

General feedback currently becomes durable only after the browser posts it to the general-comment endpoint, normally when the reviewer presses Enter.
The send controls inspect only durable draft comments.
As a result, non-empty text still in the persistent textarea is invisible to submission and an empty **Send and End** request can close the review.

Batch creation and review ending already share one SQLite transaction and one idempotency key.
The fix should preserve that atomicity rather than performing a separate browser request before submission.

## Goals / Non-Goals

**Goals:**

- Include pending general text in the same idempotent submission as durable drafts.
- Keep review ending atomic with creation of the final general feedback item.
- Preserve user input on validation, confirmation, network, or database failure.
- Keep Enter-to-queue behavior unchanged.

**Non-Goals:**

- Automatically save text from contextual comment composers.
- Change batch ordering for existing drafts.
- Add a database migration or another endpoint.
- Change the general feedback size limit or Markdown rendering.

## Decisions

### Extend the submission contract

Add an optional `general_body` field to `Submission`.
Default it to absent for compatibility with existing tests and connector-independent API clients.

The browser sends the trimmed general textarea value with **Send** or **Send and End**.
The submission signature and idempotency request JSON include this field, so an ambiguous retry carries the same body and key.

### Create general feedback inside the submission transaction

Keep annotated and already queued drafts as the existing snapshot inputs.
Inside `Store::submit`, validate optional general text and include it in the total item count.

When present, the transaction:

1. Ensures the shared general-feedback document exists.
2. Creates one comment already linked to the new batch.
3. Appends its chronological feed entry.
4. Builds the general `BatchItem` snapshot.
5. Inserts that snapshot after the explicitly queued draft items.
6. Ends the review when requested.

This keeps comment creation, batch creation, item snapshots, and the end marker atomic.
Replay returns the original submission before creating any new comment, preventing duplicates.

### Treat pending text as sendable UI state

Update controls whenever the general textarea changes.
Enable **Send** when there is at least one queued draft or a non-empty trimmed general value.

`sendDrafts` reads the pending body once at action start and places it in the retry state.
It clears the textarea only after a successful submission response.
If stale-context confirmation is required for another draft, the pending text remains visible while the reviewer decides.

The existing Enter path continues posting a standalone durable draft and clearing on success.

### Preserve explicit empty-review ending

When both durable drafts and trimmed general text are empty, **Send and End** continues using the existing empty-end endpoint.
Whitespace-only text does not create feedback or block an empty end.

## Risks / Trade-offs

- **A retry could duplicate the inline general item** -> Replay by submission key before any insertion and include `general_body` in the serialized request identity.
- **A failed send could clear the textarea prematurely** -> Clear only after the submit response succeeds.
- **An existing draft can require stale confirmation while general text is pending** -> Keep the textarea unchanged and reuse the same idempotency payload after confirmation.
- **Two creation paths can drift** -> Share general comment validation, document identity, anchor, feed insertion, and snapshot construction helpers in the review store.

## Migration Plan

No database migration is required.
Older request payloads omit `general_body` and retain their previous behavior.
The browser starts using the optional field as soon as the rebuilt embedded assets are served.
