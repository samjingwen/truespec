# Review Handoff Delta

## ADDED Requirements

### Requirement: Show agent delivery progress

TrueSpec SHALL show whether submitted feedback is waiting for the agent or handled.
Source-freshness warnings SHALL remain independent from agent progress.
The browser SHALL NOT expose a separate reviewer-resolution state.

#### Scenario: Display agent working

- **WHEN** feedback is queued, delivered, retried, or being handled
- **THEN** the browser shows a continuously rotating spinner with an accessible state label

#### Scenario: Display agent done

- **WHEN** an agent reply or recorded completion handles feedback
- **THEN** the spinner is replaced by an accessible green done icon
- **AND** any **Outdated** or source-missing warning remains visible

### Requirement: Mirror agent replies

A coding-harness connector SHALL append one durable Markdown reply for a delivered batch using a stable harness message ID.
The browser SHALL show that reply chronologically after the batch comments and link it to every comment in the batch.

#### Scenario: OpenCode completes a TrueSpec turn

- **WHEN** the paired OpenCode turn reaches idle with a completed assistant response
- **THEN** the plugin stores the final assistant Markdown as one agent reply bubble for that batch
- **AND** the browser replaces their working spinner with an accessible green done icon

#### Scenario: Retry response mirroring

- **WHEN** the connector repeats the same harness message ID and body after an ambiguous result
- **THEN** TrueSpec accepts the retry without duplicating the feed entry
- **AND** a different reply for that batch is rejected

#### Scenario: Receive a reply while Review is collapsed

- **WHEN** a new agent reply arrives while the Review panel is closed
- **THEN** the Review edge tab shows an unread indicator
- **AND** opening Review clears the indicator

## REMOVED Requirements

### Requirement: Distinguish delivery from reviewer resolution

**Reason**: Reviewer resolution duplicates the agent progress state and adds controls that are not needed to send, end, or understand a review.

**Migration**: Use the working spinner, green done icon, and source-freshness warning as the complete visible status.

### Requirement: Mirror one agent reply per batch

**Reason**: Reply mirroring remains supported, but its accepted behavior is replaced by **Mirror agent replies** without reviewer-resolution semantics.

**Migration**: Use the replacement **Mirror agent replies** requirement.
