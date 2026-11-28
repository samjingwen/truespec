# Tasks

## 1. Add private tailnet configuration and command boundary

- [x] 1.1 Add a `tailnet` module for bounded Tailscale CLI execution, JSON discovery, DNS-origin validation, Serve status parsing, and sanitized errors; verify unit tests cover missing CLI, disconnected state, malformed output, timeout, output limits, and redaction.
- [x] 1.2 Advance private service configuration with backward-compatible v1 loading and atomic v2 persistence for disabled/enabled tailnet mode, HTTPS port, origin, target, and ownership fingerprint; verify owner-only modes, malformed configuration refusal, and no user-identity persistence.
- [x] 1.3 Implement deterministic fake-Tailscale fixtures for `status --json`, `serve status --json`, Serve enable, exact-port disable, collisions, schema drift, and command failures; verify tests never require or modify the host's real Tailscale configuration.
- [x] 1.4 Update AGENTS ownership for the tailnet module and document the optional Tailscale runtime boundary in README architecture and security sections; verify paths, command names, and privacy claims match the planned implementation.

## 2. Integrate tailnet mode with service lifecycle

- [x] 2.1 Add `truespec up --tailnet` and an explicit HTTPS-port option while preserving the public command set; verify help, JSON output, default local behavior, and port validation in CLI tests.
- [x] 2.2 Implement tailnet preflight that discovers the exact HTTPS origin and refuses an occupied or unknown Serve listener before mutation; verify fixed-port collision, idempotent matching configuration, and unrelated Serve preservation with fake-CLI tests.
- [x] 2.3 Persist desired tailnet mode and perform checked local restart/reconciliation when browser trust changes; verify connector-driven recovery restores a previously enabled exact Serve listener without creating duplicate configuration.
- [x] 2.4 Extend authenticated health and status with safe tailnet enabled, port, origin, and verified fields; verify human and JSON status omit identities, credentials, repository paths, and complete review URLs.
- [x] 2.5 Implement checked `down` cleanup that disables only the exact TrueSpec-owned Serve listener and leaves mismatched configuration untouched with a recovery command; verify local shutdown, cleanup failure, registration replacement, and unrelated Serve routes.
- [x] 2.6 Document `up --tailnet`, dedicated-port selection, status, down, ACL expectations, identity behavior, persistence, and troubleshooting in README; verify every documented command matches CLI help and the installed Tailscale command syntax.

## 3. Enforce the remote browser trust boundary

- [x] 3.1 Extend browser request classification for the exact local or configured tailnet Host and same-origin mutation Origin while requiring a non-empty Serve user identity remotely; verify allowed local, allowed tailnet, missing identity, wrong Host, wrong Origin, and forged-header cases.
- [x] 3.2 Reject all tailnet `/__control/*` requests before bearer processing while preserving authenticated loopback control operations; add adversarial tests for health, registration, connector, and shutdown paths.
- [x] 3.3 Keep assets, workspace APIs, durable document links, browser history, refresh, comments, batches, and replies origin-relative through the tailnet URL; extend browser fixtures for direct navigation and desktop/phone review behavior under a simulated tailnet origin.
- [x] 3.4 Ensure remote identity values are never persisted or logged and tailnet access never broadens document discovery; verify database, registry, browser payload, status, and bounded-log audits.

## 4. Advance connector attachment and OpenCode integration

- [x] 4.1 Advance the hidden connector protocol to version 3 and return selected URL, local URL, and verified optional tailnet metadata; verify unsupported integrations fail before connector or review-state mutation.
- [x] 4.2 Generate workspace-scoped local and tailnet URLs from one review binding and return tailnet only after exact Serve verification; verify service recovery, origin changes, old tabs, and two-project isolation.
- [x] 4.3 Update the OpenCode plugin to validate exact protocol-3 local or tailnet attachment metadata, open the selected URL, and retain only local workspace binding data; verify actionable forced-refresh guidance and no control token or Tailscale identity exposure.
- [x] 4.4 Extend isolated OpenCode coverage for two repositories using selected tailnet URLs, stable delivery IDs, movement checks, mirrored replies, restart reconciliation, and clean ended-review drains without cross-workspace routing.

## 5. Verify real Tailscale Serve behavior and complete review

- [x] 5.1 Snapshot the host's existing Serve configuration, execute a real `up --tailnet` acceptance flow on a dedicated unused HTTPS port, verify access and identity from another tailnet device or equivalent authenticated request path, then restore the exact prior configuration and record limitations.
- [x] 5.2 Execute local and tailnet review URLs concurrently across two disposable repositories, exchange feedback and replies, restart the daemon, and verify local state ownership, origin isolation, and recovery.
- [x] 5.3 Run formatting, Clippy, Rust tests, strict OpenSpec validation, build, browser, isolated OpenCode, Impeccable detector where UI files change, public-repository audit, and exact-head Hardlook; address findings and rerun every invalidated check.

## Workflow follow-up

- Sync all four capability deltas and archive this change after implementation review.
- Require hosted `Quality` and `Browser and OpenCode` checks before merge.
