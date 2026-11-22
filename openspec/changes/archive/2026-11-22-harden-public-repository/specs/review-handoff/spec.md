# Review Handoff Delta

## MODIFIED Requirements

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
