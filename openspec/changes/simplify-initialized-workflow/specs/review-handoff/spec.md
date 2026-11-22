# Review Handoff Delta

## ADDED Requirements

### Requirement: Pair an initialized coding conversation

A selected integration SHALL pair one existing coding conversation with the repository's open review using initialized project configuration.
Pairing SHALL NOT launch an arbitrary agent or redirect an open review to another conversation.

#### Scenario: Start an initialized OpenCode review

- **WHEN** the user runs `/truespec-review` in an initialized repository with the OpenCode integration installed
- **THEN** the plugin starts or reuses the configured TrueSpec reader and pairs it with that exact chat and worktree

#### Scenario: Reopen from the paired conversation

- **WHEN** the user runs the integration command again in the paired conversation while its review is open
- **THEN** the same browser review opens and no new coding conversation starts

#### Scenario: Another integration requests the open review

- **WHEN** another integration or conversation tries to pair with the same repository while its review is open
- **THEN** TrueSpec rejects the pairing and preserves the existing destination and queued feedback

### Requirement: Deliver feedback through the bundled skill

Each selected integration SHALL install the portable `truespec-review` skill and invoke it for delivered TrueSpec batches.
The integration SHALL keep transport, receipt, retry, and response mirroring outside the skill.

#### Scenario: Deliver one review batch

- **WHEN** the connector leases a batch for its paired conversation
- **THEN** it queues a concise message that invokes `truespec-review` and includes the immutable batch and comment data
- **AND** it does not repeat the reusable interpretation and workflow instructions in that message

#### Scenario: Handle source-aware feedback

- **WHEN** the agent loads `truespec-review` for a delivered batch
- **THEN** the skill directs it to treat quotes as context, inspect current source, answer questions without unnecessary edits, apply explicit change requests, and clarify ambiguous intent
- **AND** the agent reports its final response in the paired conversation for automatic mirroring

## MODIFIED Requirements

### Requirement: Submit durable feedback snapshots

TrueSpec SHALL commit a batch before reporting successful submission, even when its configured integration is temporarily unavailable.
Each item SHALL carry stable identifiers, the submitted comment, the real source locator, the original anchor evidence, and the source revision and anchor status at submission.
Later edits SHALL NOT rewrite submitted evidence.

#### Scenario: Submit cross-document feedback with no agent connected

- **WHEN** the reviewer sends drafts from two documents while the configured integration cannot receive them
- **THEN** TrueSpec shows the batch as queued, and the paired integration can retrieve both items with their original context later

#### Scenario: A draft has become stale

- **WHEN** the reviewer attempts to submit a draft whose source is missing or whose anchor needs review
- **THEN** TrueSpec clearly identifies the affected draft and requires confirmation before sending its original context

## REMOVED Requirements

### Requirement: Reliable agent delivery

**Reason**: Manual inbox commands and copied browser prompts are removed from the product workflow because initialized reviews always use a selected integration.

**Migration**: Run `truespec init`, select one or more integrations, and start the review from a selected coding harness.

### Requirement: Pair an existing coding conversation

**Reason**: Pairing now requires initialized project source configuration and an installed integration rather than optional setup against an unconfigured workspace.

**Migration**: Initialize the repository and use the replacement **Pair an initialized coding conversation** requirement.
