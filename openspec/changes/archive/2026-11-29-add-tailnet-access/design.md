# Design

## Context

See `proposal.md` for motivation.
TrueSpec currently binds one Axum listener to IPv4 loopback, derives an exact local Host and Origin from that listener, and rejects every other browser origin.
The installed Tailscale CLI is optional at runtime and Tailscale Serve can terminate HTTPS, apply tailnet access rules, inject verified user identity headers, and reverse proxy to an `http://127.0.0.1` target.
The public command surface must remain `init`, `up`, `status`, and `down`.

## Goals / Non-Goals

**Goals:**

- Let `up --tailnet` expose workspace-scoped review pages to trusted devices on the same tailnet.
- Preserve loopback browser access and the authenticated local control plane.
- Let TrueSpec own and reconcile one dedicated Serve HTTPS listener without disturbing other Serve configuration.
- Return a stable tailnet review URL from connector attachment while tailnet mode is active.
- Keep Tailscale identity, source paths, control credentials, and review contents out of global logs and routing metadata.

**Non-Goals:**

- Add reviewer accounts, hosted state, browser source editing, or multi-user collaboration semantics.
- Move coding-agent execution, provider credentials, source Markdown, or SQLite review state to another device.
- Mount TrueSpec under a path prefix in the first version.
- Manage arbitrary Tailscale Services or reset the device's complete Serve configuration.

## Decisions

### Use Tailscale Serve as a private HTTPS reverse proxy

Tailnet mode keeps the existing service bound to loopback and configures one Serve HTTPS listener whose target exactly matches the authenticated local runtime registration.
The default dedicated HTTPS port is `8443`, with an explicit CLI override for collisions or local policy.
A dedicated port allows root-path routing for TrueSpec's existing absolute asset and API paths and avoids replacing unrelated configuration on the ordinary HTTPS port.

The implementation invokes the installed Tailscale CLI with bounded timeouts and output limits.
It discovers the device DNS name from `tailscale status --json`, inspects the current Serve configuration, and accepts only recognized schemas.
It configures the listener with the equivalent of:

```sh
tailscale serve --bg --yes --https=<port> http://127.0.0.1:<service-port>
```

Alternatives considered:

- A path mount such as `/truespec` was rejected because the current browser uses root-relative assets, APIs, and durable document links.
- A separate application gateway was deferred because Serve already supplies private HTTPS, access-control enforcement, and identity headers for this tailnet-only goal.
- Manual-only proxy setup was rejected because connector URL selection, status, collision safety, and exact cleanup still require TrueSpec to understand ownership.

### Persist desired tailnet mode in private service configuration

The owner-only service configuration advances version and stores whether tailnet mode is enabled, the dedicated HTTPS port, the discovered HTTPS origin, and the exact local proxy target or equivalent ownership fingerprint.
It never stores a Tailscale user identity.

`up --tailnet` preflights Tailscale before changing service state.
Tailnet transitions, configuration writes, maintenance pauses, and cold local recovery are serialized by an owner-only lifecycle lock.
Tailscale subprocesses inherit that lock so another command cannot clear ownership while an external mutation survives its initiating TrueSpec process.
If the local daemon was started without the configured origin, the command performs a checked restart under lifecycle ownership so browser trust and attachment URL generation change together.
It records the exact origin and target before invoking Serve, starts and authenticates the backend with that policy, and only then publishes the listener.
An interrupted command therefore leaves enough private ownership state for `up` or `down` to reconcile the exact listener.
After an unexpected daemon restart, connector attachment reconciles the persisted owned Serve listener before returning a tailnet URL, while polling and reply delivery require only local service readiness.

`down` preflights only the listener whose port and target match the recorded ownership, performs the normal checked local shutdown, removes that exact listener, and then disables persisted tailnet mode.
An already-absent listener is a verified idempotent cleanup and allows the stale ownership record to be cleared.
If listener cleanup cannot be verified, the local service still stops, the listener and ownership record remain unchanged, and the command returns an actionable exact-port recovery instruction.
After successful `down`, the desired tailnet mode is disabled so the next ordinary `up` remains local unless `--tailnet` is requested again.
Maintenance stops used by repository-state reset preserve the listener and desired mode before reconciling them after restart.
Legacy version-1 configuration is read without eager rewriting so a still-running previous service can be stopped by its matching binary.

### Trust exactly one tailnet browser origin

Browser middleware distinguishes:

- local browser requests using the exact loopback Host and local same-origin mutation Origin;
- tailnet browser requests using the exact configured HTTPS Host, exact HTTPS mutation Origin, a non-empty `Tailscale-User-Login` header, and the proxy-owned forwarding headers added by Serve.

Tailnet Serve removes spoofed identity headers before injecting its verified values.
Serve also replaces the forwarding headers used to distinguish proxied requests from direct loopback requests.
TrueSpec trusts these headers only because the backend remains loopback-only and the remote Host must exactly match private configuration.
Tagged-device requests without a user identity are rejected for browser access.
Identity values are not written to logs, databases, browser state, or status output.

All tailnet requests to `/__control/*` are rejected before bearer authentication.
Local control clients retain the existing loopback Host, bearer credential, protocol, bounded body, and timeout requirements.
No CORS support is added.

### Keep local and tailnet URLs for one review

The browser application remains origin-relative, so the same workspace and review path works through local and tailnet origins.
Connector attachment advances to protocol version 3 and returns:

- the selected `url` that the integration should open;
- the local loopback URL for diagnostics and recovery;
- the verified tailnet origin or tailnet URL only when mode is healthy.

The OpenCode plugin accepts local HTTP only for `127.0.0.1` and tailnet HTTPS only when it exactly matches the versioned attachment metadata.
It never discovers Tailscale, reads Serve configuration, or receives the private control token.
Existing integrations fail before attachment mutation and instruct the user to rerun forced initialization.

### Keep lifecycle health local and remote readiness explicit

Core health remains an authenticated loopback control operation.
Tailnet startup additionally verifies:

- Tailscale is connected and reports a valid DNS name;
- the chosen port is unused or already matches TrueSpec's exact target;
- Serve configuration succeeds;
- an HTTPS request through the tailnet origin reaches a browser route and carries Tailscale identity.

The tailnet readiness probe must not expose or call private control routes.
Status reports local service state plus a safe optional tailnet object containing enabled state, HTTPS origin, port, and verified status.
It omits user identity and complete review URLs.

### Treat Serve configuration as external shared state

Every mutation uses preflight, exact comparison, and post-change verification.
TrueSpec never runs `tailscale serve reset`.
The local single-user lifecycle assumes no other process modifies TrueSpec's selected Serve port concurrently with `up` or `down`.
This keeps the integration on the supported Tailscale CLI instead of depending on the unstable platform-specific LocalAPI.
CLI failures retain bounded sanitized phase diagnostics and preserve the last verified local service behavior.
Tests use a deterministic fake Tailscale executable for schema, collision, timeout, command, rollback, and redaction coverage.
A manual acceptance test uses the installed Tailscale client and restores the exact prior Serve configuration afterward.

## Risks / Trade-offs

- [The dedicated HTTPS port is already owned] -> Refuse without mutation and allow an explicit alternate port.
- [The Tailscale CLI output schema changes] -> Parse versioned known fields, reject unknown structure, and keep local mode available.
- [Serve succeeds but the remote origin is not reachable] -> Require a bounded HTTPS browser-route probe before reporting tailnet readiness.
- [Identity headers are absent for tagged devices] -> Require a user identity for browser access and document this behavior.
- [A local process can forge proxy headers] -> Keep the backend on loopback; same-user local processes already share the local trust boundary.
- [Tailnet mode restart interrupts open local tabs briefly] -> Preserve repository state and stable workspace/review paths so polling reconnects.
- [Cleanup could affect unrelated Serve state] -> Record an ownership fingerprint and disable only an exact port-and-target match.
- [A manual same-port Serve change races TrueSpec between inspection and mutation] -> Treat concurrent external mutation of TrueSpec's selected port as unsupported in the single-user local environment.
- [A tailnet hostname is a private identifier] -> Show it only in explicit status and attachment output, never in logs or committed files.

## Migration Plan

1. Add private tailnet configuration, Tailscale command preflight, ownership comparison, and fake-CLI tests.
2. Add `up --tailnet` and configurable dedicated HTTPS port without adding a public command.
3. Extend runtime browser trust to the exact configured tailnet origin and reject remote control routes.
4. Advance connector protocol to version 3 and update OpenCode URL validation and generated artifacts.
5. Add tailnet-aware status, checked cleanup, restart recovery, and rollback tests.
6. Add browser and OpenCode coverage for remote-origin routing and identity enforcement.
7. Run a manual Tailscale Serve acceptance flow on the current host, restore prior Serve configuration, and document verified behavior.
8. Update README setup, lifecycle, security, troubleshooting, and integration-refresh guidance.

Rollback disables the exact TrueSpec-owned Serve listener, returns service configuration to local mode, and uses the previous local browser origin.
Binary rollback requires refreshing the generated integration because connector protocol version 3 is intentionally breaking.
