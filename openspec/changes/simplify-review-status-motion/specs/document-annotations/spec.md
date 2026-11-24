# Document Annotations Delta

## MODIFIED Requirements

### Requirement: Persistent cross-document review feed

The review sidebar SHALL show one chronological feed without separate Drafts and History views.
Each item SHALL show its document path and target quote or original heading, with the source line number for line comments.
Reviewers SHALL be able to jump to the target and edit or remove unsent drafts.
Submitting SHALL update an item in place with its delivery and source-freshness state rather than moving it elsewhere.
Submitted items SHALL NOT expose reviewer resolution controls or resolution badges.

#### Scenario: Review drafts across documents after restart

- **WHEN** a reviewer saves drafts on different documents and restarts TrueSpec
- **THEN** the feed still shows both drafts in their original order with each document path
- **AND** navigating to one draft does not discard the other

#### Scenario: Send without reordering comments

- **WHEN** the reviewer sends several drafts and later adds another draft
- **THEN** the submitted items retain their feed positions with updated status
- **AND** the new draft appears at the end of the same feed

#### Scenario: Expand long context in the review feed

- **WHEN** expanded original context makes the chronological feed taller than the Review panel
- **THEN** the feed scrolls independently
- **AND** the general feedback box and send controls remain reachable without growing the whole panel

#### Scenario: Read submitted feedback status

- **WHEN** feedback has been submitted
- **THEN** the feed shows agent progress and any source-freshness warning
- **AND** it does not show **Resolve**, **Reopen**, **Resolved**, or **Unresolved**
