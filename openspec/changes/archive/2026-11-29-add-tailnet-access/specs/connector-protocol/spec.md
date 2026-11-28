# Connector Protocol Delta

## MODIFIED Requirements

### Requirement: Keep browser identity separate from connector identity

Shared-service attachment SHALL report one workspace-scoped browser URL using the selected loopback or verified tailnet origin.
The URL SHALL contain opaque workspace ID and review ID and SHALL NOT expose the canonical workspace path, connector destination, connection ID, control credential, or Tailscale user identity.
An old browser tab SHALL remain on its original workspace, review, and origin after another review begins.

#### Scenario: Open a paired review

- **WHEN** a connector attaches or reuses a workspace review in local mode
- **THEN** it opens the reported loopback URL with opaque workspace and review identity
- **AND** the URL reveals no filesystem path or coding-conversation identifier

#### Scenario: Open reviews for two projects

- **WHEN** two project integrations open their reported URLs against the shared listener
- **THEN** each browser resolves documents, comments, and replies only through its own workspace and review

#### Scenario: Open a paired review in tailnet mode

- **WHEN** a connector attaches or reuses a workspace review while tailnet mode is active
- **THEN** attachment returns the exact configured HTTPS tailnet origin with the workspace-scoped review path
- **AND** the connector opens that URL without reading or receiving the private local control credential

#### Scenario: Reattach after local service recovery

- **WHEN** a connector restarts the local service while persisted tailnet mode remains enabled
- **THEN** attachment verifies the owned Serve configuration before returning the tailnet URL
- **AND** it does not silently return an unverified remote origin

#### Scenario: Use an integration from the previous protocol

- **WHEN** an installed integration supports only the previous connector protocol
- **THEN** TrueSpec refuses attachment before changing review state
- **AND** it tells the user to refresh the integration with `truespec init --integration opencode --force`
