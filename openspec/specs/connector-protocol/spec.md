# connector-protocol Specification

## Purpose
Define the versioned local contract that coding-harness integrations use to pair a conversation, deliver TrueSpec feedback, and mirror the completed agent response.

## Requirements

### Requirement: Keep connectors independent from document sources

TrueSpec SHALL expose coding-harness delivery through a local connector protocol without requiring provider code or credentials in the browser or review core.
The selected document source SHALL control only document discovery and validation.
Manual inbox delivery SHALL remain available when no connector is paired.

#### Scenario: Pair a connector with an OpenSpec review

- **WHEN** a connector attaches one coding conversation and selects the OpenSpec document source
- **THEN** the destination identifies the conversation while the source independently identifies the documents
- **AND** changing connector implementations does not change document discovery behavior

### Requirement: Version and isolate connector attachment

Every connector operation SHALL require a supported protocol version.
Attaching SHALL accept an opaque stable destination, a connector kind, a display label, and a document source.
It SHALL return the protocol version, canonical workspace, connection ID, and review ID.
An unsupported version SHALL fail without changing review state.

#### Scenario: Reattach the same destination

- **WHEN** the same destination attaches again while its workspace review is open
- **THEN** TrueSpec reuses the existing connection and review

#### Scenario: Another destination tries to attach

- **WHEN** a different destination tries to attach while the workspace review is open
- **THEN** TrueSpec rejects the attachment without redirecting existing or pending work

#### Scenario: Pair after manual submission

- **WHEN** feedback was submitted before a connector attached
- **THEN** those batches remain in the manual inbox
- **AND** only later paired batches belong to the connector

### Requirement: Keep browser identity separate from connector identity

JSON viewer startup SHALL report the URL, canonical workspace, review ID, and protocol version.
A review-specific browser URL SHALL use the review ID and SHALL NOT expose the connector destination or connection ID.
An old browser tab SHALL remain on its original review after a later review begins.

#### Scenario: Open a paired review

- **WHEN** a connector starts or reuses the workspace viewer
- **THEN** it opens the reported URL with the review ID
- **AND** the URL reveals no coding-conversation identifier

### Requirement: Lease immutable batches in order

Connector retrieval SHALL return a versioned envelope with a `batch`, `idle`, or `ended` event.
A batch event SHALL include the batch ID, lease token, lease expiry, attempt number, session-ended state, and immutable comment snapshots.
TrueSpec SHALL deliver connector batches in submission order to their paired connection.
A connector lease SHALL prevent another connector and the manual inbox from claiming the same batch.

#### Scenario: Claim the next paired batch

- **WHEN** a connection retrieves pending work
- **THEN** it receives the oldest undelivered batch for that connection with an expiring lease
- **AND** another claimant cannot receive that batch while the lease remains valid

#### Scenario: Retry after an interrupted claim

- **WHEN** a connector stops without recording delivery or deferring the attempt
- **THEN** the lease eventually expires and a later attempt can claim the same immutable batch

### Requirement: Record delivery only after destination acceptance

A connector SHALL record delivery only after its destination accepts the batch.
The receipt SHALL identify the accepted harness message or turn.
Repeating the same receipt SHALL be idempotent.
A different receipt or stale lease SHALL NOT replace a recorded delivery.
A delivery receipt SHALL mean only that the destination accepted the batch, not that the agent completed it or the reviewer resolved it.

#### Scenario: Retry an ambiguous receipt write

- **WHEN** a connector repeats the same delivery receipt after an ambiguous result
- **THEN** TrueSpec accepts the retry without changing the recorded delivery

#### Scenario: Submit a conflicting receipt

- **WHEN** a connector submits another receipt for an already delivered batch
- **THEN** TrueSpec rejects it and preserves the first receipt

### Requirement: Support safe harness delivery retries

A connector SHOULD use a stable idempotency key derived from the batch ID when the harness supports one.
If the harness cannot deduplicate delivery, the connector SHALL check for an earlier accepted delivery before posting again after an ambiguous failure.
The connector SHALL persist enough binding information to resume delivery to the same destination after restart.

#### Scenario: Resume after connector restart

- **WHEN** the connector restarts with queued work
- **THEN** it resumes the existing connection and delivers the batch to the original destination
- **AND** it does not create a different review or coding conversation

### Requirement: Defer temporary failures

A connector SHALL be able to release a leased attempt with a retry delay after a temporary failure.
Deferring SHALL keep the batch queued without changing its immutable snapshot.

#### Scenario: Destination is temporarily unavailable

- **WHEN** the connector defers a valid leased batch with a retry delay
- **THEN** the batch becomes claimable again after that delay
- **AND** the next delivery attempt retains the same batch and comment IDs

### Requirement: Mirror one completed reply

After a delivered harness turn completes, a connector SHALL be able to store one Markdown reply using the harness's stable assistant-message ID.
Repeating the same reply ID and body SHALL be idempotent.
A different reply for the same batch SHALL be rejected.
The browser SHALL append the reply once to the chronological feed and link it to every batch comment.
Recording a reply SHALL mark the batch items handled but SHALL NOT resolve reviewer comments.

#### Scenario: Store the completed harness response

- **WHEN** the connector records the completed response for a delivered batch
- **THEN** TrueSpec appends one linked reply after that batch's comments
- **AND** the reviewer controls resolution independently

#### Scenario: Retry response recording

- **WHEN** the connector repeats the same stable reply ID and Markdown body
- **THEN** TrueSpec accepts the retry without creating another feed entry

### Requirement: Drain an ended review without ending the coding chat

A final batch MAY report that the browser review has ended.
The connector SHALL continue retrieving queued batches until it receives the `ended` event.
The `ended` event SHALL terminate the TrueSpec delivery loop, not the coding conversation.

#### Scenario: Deliver a final batch

- **WHEN** the reviewer sends a final batch and ends the review
- **THEN** the connector delivers that batch with the ended state
- **AND** it continues until TrueSpec reports that no queued work remains
- **AND** the existing coding chat remains available
