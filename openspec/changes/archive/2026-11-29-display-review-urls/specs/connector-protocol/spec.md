# Connector Protocol Delta

## MODIFIED Requirements

### Requirement: Keep browser identity separate from connector identity

Shared-service attachment SHALL report one workspace-scoped browser URL using the selected loopback or verified tailnet origin.
The URL SHALL contain opaque workspace ID and review ID and SHALL NOT expose the canonical workspace path, connector destination, connection ID, control credential, or Tailscale user identity.
An old browser tab SHALL remain on its original workspace, review, and origin after another review begins.

#### Scenario: Open a paired review

- **WHEN** a connector attaches or reuses a workspace review in local mode
- **THEN** it reports the loopback URL with opaque workspace and review identity
- **AND** the URL reveals no filesystem path or coding-conversation identifier

#### Scenario: Open reviews for two projects

- **WHEN** two project integrations open their reported URLs against the shared listener
- **THEN** each browser resolves documents, comments, and replies only through its own workspace and review

#### Scenario: Open a paired review in tailnet mode

- **WHEN** a connector attaches or reuses a workspace review while tailnet mode is active
- **THEN** attachment returns the exact configured HTTPS tailnet origin with the workspace-scoped review path
- **AND** the connector reports that URL without reading or receiving the private local control credential

#### Scenario: Reattach after local service recovery

- **WHEN** a connector restarts the local service while persisted tailnet mode remains enabled
- **THEN** attachment verifies the owned Serve configuration before returning the tailnet URL
- **AND** it does not silently return an unverified remote origin

#### Scenario: Use an integration from the previous protocol

- **WHEN** an installed integration supports only the previous connector protocol
- **THEN** TrueSpec refuses attachment before changing review state
- **AND** it tells the user to refresh the integration with `truespec init --integration opencode --force`

## ADDED Requirements

### Requirement: Present review URLs in the coding conversation

An installed integration SHALL present the available review URLs to its paired conversation as URL lines without labels or other message text.
Presenting review URLs SHALL NOT launch a browser automatically.

#### Scenario: Present a local review URL

- **WHEN** an integration opens a review in local mode
- **THEN** its conversation message contains exactly the loopback review URL
- **AND** it contains no label or explanatory text

#### Scenario: Present tailnet and local review URLs

- **WHEN** an integration opens a review in tailnet mode
- **THEN** its conversation message contains the selected tailnet review URL on the first line and the loopback review URL on the second line
- **AND** it contains no label or explanatory text

#### Scenario: Present URLs without launching a browser

- **WHEN** an integration opens or resumes a review in local or tailnet mode
- **THEN** it leaves browser navigation under the user's control
- **AND** it does not invoke an operating-system browser launcher
