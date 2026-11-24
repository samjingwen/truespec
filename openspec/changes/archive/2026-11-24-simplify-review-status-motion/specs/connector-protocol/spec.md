# Connector Protocol Delta

## MODIFIED Requirements

### Requirement: Record delivery only after destination acceptance

A connector SHALL record delivery only after its destination accepts the batch.
The receipt SHALL identify the accepted harness message or turn.
Repeating the same receipt SHALL be idempotent.
A different receipt or stale lease SHALL NOT replace a recorded delivery.
A delivery receipt SHALL mean only that the destination accepted the batch, not that the agent completed it.

#### Scenario: Retry an ambiguous receipt write

- **WHEN** a connector repeats the same delivery receipt after an ambiguous result
- **THEN** TrueSpec accepts the retry without changing the recorded delivery

#### Scenario: Submit a conflicting receipt

- **WHEN** a connector submits another receipt for an already delivered batch
- **THEN** TrueSpec rejects it and preserves the first receipt

### Requirement: Mirror one completed reply

After a delivered harness turn completes, a connector SHALL be able to store one Markdown reply using the harness's stable assistant-message ID.
Repeating the same reply ID and body SHALL be idempotent.
A different reply for the same batch SHALL be rejected.
The browser SHALL append the reply once to the chronological feed and link it to every batch comment.
Recording a reply SHALL mark the batch items handled.

#### Scenario: Store the completed harness response

- **WHEN** the connector records the completed response for a delivered batch
- **THEN** TrueSpec appends one linked reply after that batch's comments
- **AND** the linked feedback shows the agent-done state

#### Scenario: Retry response recording

- **WHEN** the connector repeats the same stable reply ID and Markdown body
- **THEN** TrueSpec accepts the retry without creating another feed entry
