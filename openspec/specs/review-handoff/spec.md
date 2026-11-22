# review-handoff Specification

## Purpose
Let a reviewer send source-aware feedback as a durable batch and continue the discussion in the original coding agent chat.

## Requirements

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

### Requirement: End the browser review

The browser SHALL offer **Send** to submit a batch without ending and **Send and End** to submit any drafts and end in one action.
With no drafts, **Send** SHALL be disabled and **Send and End** SHALL end the review without a batch.
An ended session SHALL be read-only, preserve its history, and direct the reviewer back to the agent.
Ending SHALL NOT terminate the agent or silently discard drafts.

#### Scenario: Send and end succeeds

- **WHEN** the reviewer selects **Send and End**
- **THEN** the batch and end marker are committed together, and the browser displays **Return to your agent**
- **AND** the agent can retrieve pending work and learn the browser session ended

#### Scenario: Send and end fails

- **WHEN** the batch cannot be saved during **Send and End**
- **THEN** the session remains open, the drafts are still editable, and the reviewer can retry

#### Scenario: End an empty review

- **WHEN** the reviewer chooses **Send and End** with no pending drafts
- **THEN** the session ends without creating a batch

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

### Requirement: Distinguish delivery from reviewer resolution

TrueSpec SHALL distinguish queued, received, and completed work from the reviewer's decision to resolve an annotation.
For paired reviews, it SHALL show delivery only after the connector records that the destination accepted the batch.
It SHALL NOT require a browser conversation to complete work or automatically resolve an annotation when the agent completes it.

#### Scenario: Continue in agent chat

- **WHEN** the agent receives feedback for an ended browser review
- **THEN** the CLI indicates that the agent should continue in the original chat rather than wait for more browser messages
- **AND** the reviewer can later open a new review and resolve or reopen annotations independently of delivery status

### Requirement: Support questions and change requests

Review feedback SHALL support explanations, clarification, suggestions, and requested source changes in the same delivery loop.
The connector guidance SHALL NOT instruct an agent to edit Markdown merely because the reviewer asked a question.

#### Scenario: Ask for an explanation

- **WHEN** a reviewer asks a question without requesting a source change
- **THEN** the agent answers in the coding chat and the response is mirrored to TrueSpec
- **AND** Markdown remains unchanged unless the answer exposes a clear documentation defect that the agent explains

#### Scenario: Receive ambiguous intent

- **WHEN** feedback could reasonably be interpreted as either a question or a change request
- **THEN** the agent asks for clarification before editing the source

### Requirement: Mirror one agent reply per batch

A coding-harness connector SHALL be able to append one durable Markdown reply for a delivered batch using a stable harness message ID.
The browser SHALL show that reply chronologically after the batch comments and link it to every comment in the batch.
Mirroring a reply SHALL NOT resolve reviewer comments.

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
- **AND** opening Review clears the indicator without resolving any comment

#### Scenario: Distinguish agent work from reviewer resolution

- **WHEN** feedback is queued, delivered, retried, or being handled
- **THEN** the browser shows a spinner with an accessible state label
- **AND** an agent reply or recorded completion replaces it with a green done icon
- **AND** **Unresolved / Resolved** remains a separate reviewer-controlled state
