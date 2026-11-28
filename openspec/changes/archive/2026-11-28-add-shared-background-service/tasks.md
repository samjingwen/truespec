# Tasks

## 1. Establish private service lifecycle

- [x] 1.1 Add reviewed locked dependencies and modules for global paths, authenticated local HTTP clients, secure credentials, owner locking, and bounded structured logs; verify dependency policy, compilation, and focused path/permission tests.
- [x] 1.2 Implement private persistent service configuration and atomic runtime registration with instance, PID, endpoint, version, protocol, and generation identity; verify owner-only modes, redaction, malformed-file refusal, and atomic replacement tests.
- [x] 1.3 Implement authenticated health and lifecycle state reporting with bounded probes; verify stopped, starting, ready, failed, stale, incompatible, timeout, and unrelated-endpoint classifications.
- [x] 1.4 Implement contested detached startup and one-owner locking for `truespec up`; verify repeated and concurrent starts produce one service, the launcher can exit, startup failures retain bounded diagnostics, and unknown port owners are untouched.
- [x] 1.5 Implement checked graceful `truespec down` with matching-registration revalidation, bounded wait, safe escalation, and idempotent stopped behavior; verify replacement instances and unrelated PIDs are never signalled or deregistered.
- [x] 1.6 Expose `up`, `status`, and `down` in normal CLI help with human and machine-readable status output, keep the daemon entrypoint hidden, and verify command help, exit behavior, and credential/path redaction.

## 2. Add repository-owned workspace identity and routing

- [x] 2.1 Append a review-database migration for one stable random workspace ID and advance `PRAGMA user_version`; verify existing v4 databases migrate transactionally, new databases initialize once, and unrelated review rows remain unchanged.
- [x] 2.2 Add the global routing registry with only workspace ID, canonical root, configuration fingerprint, and availability metadata; verify its schema contains no review-domain tables or source content and can be rebuilt from repository state.
- [x] 2.3 Implement trusted workspace registration and revalidation through the private control API; verify Git root, canonical source config, symlink containment, root uniqueness, and no browser-selected path authority.
- [x] 2.4 Implement duplicate-state and moved-root handling; verify two available roots with one copied workspace ID are refused, a verified move can rebind only after the old root is unavailable, and a fresh clone receives a different ID.
- [x] 2.5 Implement lazy project-context loading, configuration fingerprint invalidation, safe cache eviction, and unavailable-project reporting; verify one broken root does not affect other registered projects.

## 3. Serve multiple project reviews through one listener

- [x] 3.1 Refactor the web application state from one store/source/workspace to workspace-resolved project contexts; verify every document and review handler selects exactly one validated context.
- [x] 3.2 Add workspace-scoped browser URLs and API routes, update embedded client URL construction, and verify URLs expose neither filesystem paths nor connector destinations.
- [x] 3.3 Preserve loopback Host/Origin guards, security headers, source restrictions, safe rendering, and mutation limits across shared routes; add adversarial wrong-workspace, arbitrary-path, symlink, forged-origin, and unknown-identity tests.
- [x] 3.4 Extend browser fixtures to run concurrent reviews from two repositories on one listener; verify navigation, comments, refresh, browser history, batches, replies, and old tabs remain isolated.
- [x] 3.5 Verify service restart and lazy reload preserve both project databases, one open review per project, unavailable-root errors, and independent progress in the remaining project.

## 4. Make initialization and connectors service-aware

- [x] 4.1 Reorder `truespec init` as full file preflight, compatible-service ensure, atomic project-file application, workspace-identity creation, and registration; verify startup failure writes nothing and registration failure is idempotently repairable.
- [x] 4.2 Advance the hidden connector protocol and convert attach, poll, delivered, defer, and respond operations into authenticated service clients while preserving envelopes, leases, receipts, retries, and reply idempotency; verify incompatible versions fail before state changes.
- [x] 4.3 Update the OpenCode plugin to remove per-workspace viewer children, consume the attach-returned browser URL, ensure service recovery after initialization, and keep one bridge per connection; verify plugin abort no longer owns the daemon lifecycle.
- [x] 4.4 Update initialization artifacts and upgrade guidance so existing repositories refresh integrations with `truespec init --integration opencode --force`; verify preflight still protects differing generated files.
- [x] 4.5 Extend isolated OpenCode integration coverage to two repositories and two chats sharing one service; verify browser URLs, synthetic messages, queued deliveries, stable message IDs, mirrored replies, session movement checks, and ended-review drains cannot cross workspaces.

## 5. Verify local acceptance and document ownership

- [x] 5.1 Add a real-process lifecycle harness with isolated XDG directories; verify status-before-up, init auto-start, repeated up, concurrent up, status, graceful down, restart, stale registration, endpoint collision, incompatible version, and launcher-exit survival.
- [x] 5.2 Execute the user acceptance flow with two disposable Git repositories: initialize each, open separate OpenCode reviews, use both browser URLs concurrently, exchange feedback and replies, restart the daemon, and verify complete isolation and persistence.
- [x] 5.3 Update README, architecture tables, security/privacy guidance, CLI reference, installation upgrade steps, and AGENTS ownership for the shared service, global routing metadata, repository-owned state, and future bounded consumer contract; verify commands and paths match implementation.
- [x] 5.4 Run formatting, Clippy, Rust tests, strict OpenSpec validation, build, browser, isolated OpenCode, Impeccable detector where UI files change, desktop/phone inspection, public-repository audit, and full exact-head Hardlook; address findings and rerun invalidated evidence.

## Workflow follow-up

- Archive `compact-workspace-controls` before implementation delivery begins.
- Sync every delta spec and archive this change after implementation review.
- Plan launchd packaging, Mac Mini deployment, remote ingress, expiring links, SSE, and TenAxe integration as separate accepted changes after local multi-project service acceptance.
