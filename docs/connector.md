# Local connector protocol

TrueSpec owns the review, Markdown rendering, and durable queue.
A connector adapts that queue to a coding conversation without requiring provider code or credentials in TrueSpec's browser or core.
The optional OpenCode plugin is one implementation; manual inbox commands remain available for unpaired work.

## Pair and display a review

Run the connector command with the reviewed workspace as its working directory, or pass `--root "$WORKSPACE"`.
The `--source` option is only for document discovery and validation; it is not tied to a connector type.
The destination is an opaque, stable identifier for one chat in the connector.

```sh
truespec connector --protocol 1 attach \
  --kind example --destination "$CHAT_ID" --label "Example" --source openspec
```

The JSON response has `protocol_version`, `workspace`, `connection_id`, and `review_id`.
An unsupported protocol fails rather than changing state.
Attaching the same destination reuses the open review; a different destination is rejected until that review ends.
Work already submitted before pairing remains in its original inbox.

Start or reuse a viewer for the workspace:

```sh
truespec serve --root "$WORKSPACE" --json
```

The startup record reports `url`, `workspace`, `review_id`, and `protocol_version`.
Open the URL with `?review=<review_id>` so an old tab stays on its original review when a new one begins.
Do not include the connector's destination or connection ID in the browser URL.

## Deliver batches

```sh
truespec connector --protocol 1 poll --root "$WORKSPACE" \
  --connection "$CONNECTION_ID" --wait 20
```

The result is the versioned inbox envelope with `event: "batch"`, `"idle"`, or `"ended"`.
A batch includes `batch_id`, `lease`, `expires_at_ms`, `attempt`, and the immutable comment snapshots in `items`.
Pairing and lease checks keep other connectors and the manual inbox from claiming that work.
Deliver batches in order to the one paired destination; never switch a batch to a different chat.
Use an idempotency key derived from `batch_id` when the harness supports one.
After ambiguous failures, look for the first delivery before posting again if the harness cannot deduplicate.

Record a receipt only after the destination accepts the batch:

```sh
truespec connector --protocol 1 delivered "$BATCH_ID" --root "$WORKSPACE" \
  --connection "$CONNECTION_ID" --lease "$LEASE" --receipt "$RECEIPT"
```

Repeating the same receipt is safe.
Different receipts and stale leases cannot overwrite a recorded delivery.
A receipt means the conversation accepted the message, not that the agent finished or that the reviewer resolved a comment.

## Mirror the completed agent reply

After the harness turn completes, store one Markdown reply for the delivered batch:

```sh
truespec connector --protocol 1 respond "$BATCH_ID" --root "$WORKSPACE" \
  --connection "$CONNECTION_ID" --reply-id "$HARNESS_MESSAGE_ID" \
  --body-file reply.md
```

Use `--body-file -` to read the reply from stdin.
The reply ID must be the harness's stable assistant-message ID.
Repeating the same ID and body is safe; another reply for the same batch is rejected.
The reply is appended once to the chronological browser feed and linked to every comment in the batch.
Recording a reply marks the batch items replied or completed, but never resolves reviewer comments.

For a temporary failure, keep the batch queued and release the attempt with a delay:

```sh
truespec connector --protocol 1 defer "$BATCH_ID" --root "$WORKSPACE" \
  --connection "$CONNECTION_ID" --lease "$LEASE" --retry-after 5
```

If the process stops without deferring, the lease expires and a new attempt can claim the same batch.
Persist the binding on the connector side so delivery can resume after restart.
`session_ended: true` can accompany a final batch; continue draining until `event: "ended"`.
That terminates the delivery loop, not the coding chat.
