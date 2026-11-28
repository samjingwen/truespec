# Proposal

## Why

TrueSpec review pages are currently limited to the machine running the service, which prevents a reviewer from continuing the same local review on another trusted device.
Tailscale Serve can provide private HTTPS access within the user's tailnet while TrueSpec keeps source files, agent execution, control credentials, and review databases on the original machine.

## What Changes

- Add optional private tailnet access through `truespec up --tailnet`.
- Configure and own one dedicated Tailscale Serve HTTPS listener without replacing unrelated Serve configuration.
- Accept one exact discovered tailnet origin for browser routes while retaining the existing loopback origin.
- Require Tailscale Serve identity headers on tailnet browser requests and continue applying Tailscale access-control policy at the network boundary.
- Keep all private `/__control/*` routes local-only, even when browser routes are available through Tailscale Serve.
- Return and open a tailnet review URL when tailnet mode is active, while preserving local URLs and repository-owned state.
- Report tailnet mode and its safe browser origin through `truespec status`, and remove only TrueSpec-owned Serve configuration during `truespec down`.
- Document Tailscale setup, lifecycle, security behavior, and recovery in the README.
- **BREAKING**: Advance the hidden connector protocol so installed integrations validate private HTTPS tailnet URLs and must be refreshed with `truespec init --integration opencode --force`.

## Capabilities

### New Capabilities

- `tailnet-access`: Private Tailscale Serve discovery, identity enforcement, browser trust boundaries, and owned Serve lifecycle.

### Modified Capabilities

- `service-lifecycle`: Add idempotent `up --tailnet`, tailnet-aware status, collision refusal, and owned Serve cleanup.
- `document-reading`: Allow one workspace-scoped reader to operate through either the local loopback origin or the configured tailnet HTTPS origin.
- `connector-protocol`: Return and validate the selected local or tailnet browser URL without exposing control credentials or connector identity.

## Impact

- Affects service configuration and status schemas, CLI flags, Tailscale CLI integration, Host and Origin validation, browser URL generation, connector protocol versioning, the OpenCode plugin, README guidance, and lifecycle/browser/OpenCode tests.
- Adds an optional runtime dependency on an authenticated Tailscale CLI only when tailnet mode is requested.
- Keeps loopback mode as the default and does not move source Markdown, review state, agent execution, or private control operations off the local machine.
