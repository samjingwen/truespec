# Service Lifecycle Delta

## ADDED Requirements

### Requirement: Manage optional tailnet exposure with service lifecycle

`up` SHALL support optional tailnet exposure without adding another public command.
TrueSpec SHALL manage only the dedicated Tailscale Serve listener it has verified and recorded as its own.

#### Scenario: Start the service with tailnet access

- **WHEN** the user runs `truespec up --tailnet`
- **THEN** TrueSpec reaches authenticated local readiness before publishing its tailnet browser origin
- **AND** local lifecycle clients continue using the loopback registration and bearer credential

#### Scenario: Report tailnet service state

- **WHEN** `truespec status` inspects a healthy service with tailnet mode enabled
- **THEN** human and JSON output report that tailnet access is active and include the safe HTTPS browser origin
- **AND** they omit Tailscale user identities, private credentials, review URLs, and repository paths

#### Scenario: Stop a tailnet-enabled service

- **WHEN** the user runs `truespec down` for a matching healthy tailnet-enabled service
- **THEN** TrueSpec gracefully stops the local daemon and removes only its recorded Serve listener
- **AND** unrelated Tailscale Serve configuration remains unchanged

#### Scenario: Tailnet cleanup cannot be verified

- **WHEN** TrueSpec cannot prove that a Serve listener still matches the configuration it owns
- **THEN** it leaves that listener unchanged and reports the manual recovery command
- **AND** it does not reset or overwrite the device's complete Serve configuration

#### Scenario: Start without tailnet access after earlier use

- **WHEN** the user starts TrueSpec in default local mode after tailnet mode was previously disabled
- **THEN** the service returns local browser URLs and does not recreate Serve configuration
