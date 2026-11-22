# Document Annotations Delta

## MODIFIED Requirements

### Requirement: Adapter-independent document identity

The same workspace file exposed by different adapters SHALL retain its annotation history.
Missing paths SHALL be shown as missing rather than reassigned to a similarly named or moved file.
Submitted snapshots SHALL retain their historical document identity and source evidence.

#### Scenario: Switch document adapters

- **WHEN** a reviewer opens the same source file through a different adapter in the same workspace
- **THEN** the file shows its existing drafts and submitted annotations

#### Scenario: Move a reviewed file

- **WHEN** a reviewed file is moved within the workspace
- **THEN** its history still shows the original location and identifies the source as missing
- **AND** TrueSpec does not attach that history to the new path automatically
