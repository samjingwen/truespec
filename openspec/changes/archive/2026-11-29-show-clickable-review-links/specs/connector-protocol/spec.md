# Connector Protocol Delta

## MODIFIED Requirements

### Requirement: Present review URLs in the coding conversation

An installed integration SHALL present the available review destinations as compact Markdown links in a normal conversation message.
It SHALL admit that message without requesting an agent response and SHALL NOT launch a browser automatically.

#### Scenario: Present a local review URL

- **WHEN** an integration opens a review in local mode
- **THEN** its conversation message contains exactly `[Local](<loopback-review-url>)`
- **AND** the label links to the canonical loopback review URL

#### Scenario: Present tailnet and local review URLs

- **WHEN** an integration opens a review in tailnet mode
- **THEN** its conversation message contains exactly `[Tailnet](<tailnet-review-url>) · [Local](<loopback-review-url>)`
- **AND** each label links to its canonical review URL

#### Scenario: Admit links without an agent turn

- **WHEN** an integration presents review links
- **THEN** OpenCode stores a normal user message without issuing a model request or creating an assistant response
- **AND** the message is available to future conversation context

#### Scenario: Present URLs without launching a browser

- **WHEN** an integration opens or resumes a review in local or tailnet mode
- **THEN** it leaves browser navigation under the user's control
- **AND** it does not invoke an operating-system browser launcher
