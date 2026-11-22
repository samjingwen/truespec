# review-handoff Specification

## Purpose
Let a reviewer send source-aware feedback as a durable batch and continue the discussion in the original coding agent chat.

## Requirements

### Requirement: Submit durable feedback snapshots

TrueSpec SHALL commit a batch before reporting successful submission, even when no agent is connected.
Each item SHALL carry stable identifiers, the submitted comment, the real source locator, the original anchor evidence, and the source revision and anchor status at submission.
Later edits SHALL NOT rewrite submitted evidence.

#### Scenario: Submit cross-document feedback with no agent connected

- **WHEN** the reviewer sends drafts from two documents while no agent is polling
- **THEN** TrueSpec shows the batch as queued, and the agent can retrieve both items with their original context later

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

### Requirement: Reliable agent delivery

The agent CLI SHALL support long-running retrieval of pending feedback, explicit receipt acknowledgement, partial item completion, and one final mirrored response per manual batch.
Uncompleted work SHALL remain retrievable after an interrupted attempt.
Completed items SHALL NOT be sent again as work, and expired attempts SHALL NOT change recorded results.

#### Scenario: Agent stops after handling one item

- **WHEN** a leased batch expires after the agent acknowledges it and records only one item complete
- **THEN** the next retrieval leases the remaining items without duplicating the completed item

#### Scenario: Receive work after the viewer stops

- **WHEN** the reviewer submits a batch and stops the viewer before a manual agent receives it
- **THEN** an agent can still retrieve and complete the batch with the local CLI

#### Scenario: Listen without repeated short polls

- **WHEN** a manual agent runs `truespec listen` with the default inactivity timeout
- **THEN** it waits for feedback or review end for up to 30 minutes without requiring another command
- **AND** it exits with `idle_timeout` after 30 minutes without agent-relevant activity

#### Scenario: Mirror a manual agent response

- **WHEN** an acknowledged manual batch is handled and the agent runs `truespec reply`
- **THEN** one idempotent Markdown reply is appended to the chronological feed and all batch items are marked handled
- **AND** reviewer resolution remains unchanged

#### Scenario: Prompt an agent that is not polling

- **WHEN** an unpaired review has no agent listening for feedback
- **THEN** the viewer shows a collapsed disclosure containing a copyable prompt that identifies the review and asks the coding agent to run the listen, acknowledge, process, and reply loop
- **AND** the disclosure preserves its expanded state across document and review refreshes
- **AND** the viewer does not expose a raw short-polling command as the primary handoff

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

### Requirement: Pair an existing coding conversation

An optional connector SHALL pair a review with one existing coding conversation using a versioned local contract.
The OpenCode connector SHALL provide `/truespec-review` without arguments and use the invoking chat's OpenSpec workspace.
Pairing SHALL NOT redirect previously submitted batches or require a connector for standalone use.

#### Scenario: Reopen a paired review from the same conversation

- **WHEN** the user runs `/truespec-review` again in the paired chat while its review is open
- **THEN** the same review opens and no new coding conversation starts

#### Scenario: Install connectors interactively

- **WHEN** a user runs `truespec setup` in an interactive terminal
- **THEN** TrueSpec lists supported coding harness connectors and their installation state for selection
- **AND** selecting OpenCode installs or updates its plugin in OpenCode's global discovery directory with the current binary path
- **AND** it refuses to replace a differing plugin file unless the user explicitly passes `--force`

#### Scenario: Select connectors non-interactively

- **WHEN** setup runs without an interactive terminal
- **THEN** repeatable `--connector`, `--all`, and `--list` options support scripted discovery and installation
- **AND** setup with no selection prints available connectors and the exact selection command instead of waiting for input

#### Scenario: Iterate in the same review

- **WHEN** the user selects **Send** in the paired review
- **THEN** the batch goes to the same chat without interrupting its current work
- **AND** later Markdown changes appear in the same browser review, ready for another round

#### Scenario: Request a review in a different chat

- **WHEN** another chat tries to pair with the same workspace while its review is open
- **THEN** TrueSpec rejects the new pairing and does not redirect existing or pending feedback

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
