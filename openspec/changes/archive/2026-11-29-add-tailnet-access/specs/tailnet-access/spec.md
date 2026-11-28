# Tailnet Access Delta

## Purpose

Let a reviewer open TrueSpec review pages from other trusted devices on the same Tailscale network while all source, agent execution, control credentials, and review state remain on the host machine.

## ADDED Requirements

### Requirement: Enable private tailnet browser access

TrueSpec SHALL provide an optional tailnet mode that keeps the core service on loopback and exposes browser routes through one owned Tailscale Serve HTTPS listener.
Loopback mode SHALL remain the default.

#### Scenario: Start tailnet mode

- **WHEN** the user runs `truespec up --tailnet` on a Tailscale-connected host with an available configured HTTPS port
- **THEN** TrueSpec starts or reuses its local service and configures Tailscale Serve to proxy that dedicated HTTPS listener to the loopback service
- **AND** it reports the resulting `https://<device>.<tailnet>.ts.net:<port>` browser origin

#### Scenario: Repeat tailnet startup

- **WHEN** TrueSpec already owns a matching Serve listener for the running service
- **THEN** repeating `truespec up --tailnet` reuses the same local service, HTTPS port, and tailnet origin
- **AND** it does not create duplicate Serve configuration

#### Scenario: Encounter an occupied Serve port

- **WHEN** the requested HTTPS port is already configured for another Serve target
- **THEN** TrueSpec refuses to overwrite that configuration
- **AND** it reports an actionable port-collision error without changing the existing Serve configuration

#### Scenario: Tailscale is unavailable

- **WHEN** tailnet mode is requested but the Tailscale CLI, connection, DNS name, HTTPS support, or Serve configuration cannot be verified
- **THEN** TrueSpec reports the unmet requirement without weakening local browser or control-route security

### Requirement: Authenticate tailnet browser requests

TrueSpec SHALL accept tailnet browser traffic only for the exact configured HTTPS origin and only when Tailscale Serve supplies a user identity.
Tailnet access SHALL remain subject to the tailnet's access-control policy.

#### Scenario: Open a review from another tailnet device

- **WHEN** an allowed tailnet user opens a TrueSpec review URL through the configured Serve origin
- **THEN** the browser can load that workspace and use the same review operations available through loopback
- **AND** the backend accepts the exact tailnet Host and same-origin mutation Origin

#### Scenario: Reject a request without Serve identity

- **WHEN** a request uses the tailnet Host but lacks the required Tailscale Serve user identity
- **THEN** TrueSpec rejects the request without serving review data

#### Scenario: Reject another remote origin

- **WHEN** a browser request uses a Host or mutation Origin other than the exact loopback or configured tailnet origin
- **THEN** TrueSpec rejects the request

### Requirement: Keep the control plane local

Tailnet mode SHALL expose only embedded browser assets and workspace-scoped browser APIs.
Private lifecycle, workspace registration, connector, health, and shutdown routes SHALL remain accessible only through the authenticated loopback control plane.

#### Scenario: Request a control route through Serve

- **WHEN** a tailnet request targets any `/__control/*` route
- **THEN** TrueSpec rejects it even if the request has a valid Tailscale identity
- **AND** the private bearer credential is neither required from nor disclosed to the remote browser

#### Scenario: Use the local control client while tailnet mode is active

- **WHEN** the CLI or installed integration invokes an authenticated control operation over loopback
- **THEN** the operation continues to use the private local bearer credential and existing protocol checks

### Requirement: Preserve local ownership of reviewed data

Tailnet access SHALL transport rendered review UI and review operations without moving source Markdown, review databases, connector delivery, agent execution, or provider credentials off the host machine.

#### Scenario: Review from a tailnet device

- **WHEN** a reviewer reads documents, submits comments, or receives mirrored agent replies through the tailnet URL
- **THEN** source reads and review-state writes occur against the registered repository on the host machine
- **AND** the coding harness continues running in its original local environment
