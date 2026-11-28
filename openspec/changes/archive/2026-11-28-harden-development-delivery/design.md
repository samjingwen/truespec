# Design

## Context

See `proposal.md` for motivation.
TrueSpec is a public, connector-neutral Rust project whose local verification also uses OpenSpec, Node.js, `chrome-devtools-axi`, and OpenCode.
The primary checkout is `master`, completed changes are expected to merge through GitHub, and OpenCode can move an active session between worktrees.
TrueSpec currently has no GitHub Actions workflow or branch protection.

## Goals / Non-Goals

**Goals:**

- Make every repository edit start in an isolated Worktrunk worktree.
- Make local verification, independent review, pull-request delivery, merge verification, local `master` synchronization, and session return one explicit sequence.
- Run the complete ordinary and integration gates as two required GitHub checks.
- Match FreeQuantCamp's effective `master` restrictions while using TrueSpec-specific check names.
- Keep the workflow usable by coding harnesses other than OpenCode.
- Define how future harness-specific workspace transitions can be added without changing the shared delivery sequence.

**Non-Goals:**

- Add release, deployment, production, runtime-refresh, or package-publication automation.
- Add Worktrunk hooks that mutate a new worktree automatically.
- Require an approving GitHub review that the repository's single maintainer cannot provide to their own pull request.
- Change TrueSpec product behavior or runtime dependencies.
- Remove completed worktrees automatically.

## Decisions

### Keep shared policy in `AGENTS.md` and isolate harness-specific transitions

`AGENTS.md` will remain the canonical policy because TrueSpec supports multiple coding harnesses and OpenCode discovers it without additional configuration.
The delivery sequence will describe a harness-neutral transition contract first: use a verified native workspace-move operation when the active harness provides one, invoke it at a safe boundary, and verify the destination in a later operation.
Harness-specific entries will then map that contract to an exact verified tool without changing the shared worktree, review, merge, or synchronization rules.
The OpenCode instruction will name the current Code Mode path, `tools.opencode.session.move`, require the agent to follow the injected tool schema rather than guess its arguments, and require a blocker report when the tool is unavailable.
OpenCode itself supplies this tool, so a repository config file neither defines nor enables it.
A separate `opencode.json` instruction would duplicate the policy, make the core workflow OpenCode-specific, and risk retaining obsolete tool syntax.
Support for a new harness will require a verified native transition mechanism, its exact invocation boundary, and a post-transition verification procedure.
When no verified native transition exists, the agent will remain in the implementation worktree and report the limitation instead of substituting shell `cd`, Worktrunk shell integration, or a new harness process.

### Require a Worktrunk worktree for every edit

Planning and read-only investigation may occur in the primary checkout, but any tracked repository edit requires a dedicated branch and worktree created from the current default branch after fetching the remote.
The agent records the primary checkout path before moving and never edits, stashes, resets, cleans, rebases, or force-updates user-owned work to complete the workflow.
This stricter rule avoids ambiguous exceptions for "small" documentation or policy changes.

### Use GitHub pull requests as the merge boundary

Implementation branches are committed, pushed, and opened as pull requests.
The pull request records decisions, exact-head checks, Hardlook coverage for implementation changes, limitations, and migration or release notes when relevant.
Merging occurs through GitHub after the actual required checks and conversations are complete.
Release authority remains separate and must be explicit.

### Split hosted verification into two required checks

`.github/workflows/ci.yml` will run for pull requests and pushes to `master` with read-only repository permissions.
The `Quality` job runs formatting, Clippy, Rust tests, and strict OpenSpec validation.
The `Browser and OpenCode` job builds the binary and runs both Node integration suites.
Both jobs run on every pull request so branch protection always receives stable check contexts.

GitHub Actions will be pinned to complete commit SHAs.
CI tools will use explicit versions aligned with the locally verified toolchain: Rust 1.95.0, Node.js 26.13.1, OpenSpec 2.1.0, OpenCode 2.4.2, `chrome-devtools-axi` 0.1.46, and `chrome-devtools-mcp` 1.14.0.
These installations do not introduce runtime dependencies or a repository Node package manifest.

### Reproduce FreeQuantCamp's protection semantics with TrueSpec checks

Classic branch protection will require a pull request, strict `Quality` and `Browser and OpenCode` checks, administrator enforcement, and resolved conversations.
It will block force pushes and branch deletion.
It will retain FreeQuantCamp's zero required approvals, disabled signed-commit requirement, disabled linear-history requirement, disabled Code Owner review, and disabled last-push approval.
The bootstrap pull request must report both status contexts successfully before those contexts are made mandatory.

### Synchronize and verify the primary checkout after merge

After GitHub reports a merge, the agent records the merge SHA, inspects the primary checkout for collisions, fetches the remote, and performs only a fast-forward pull.
It verifies that local `master` equals the freshly fetched `origin/master` and contains the merge.
OpenCode then calls its session-move tool alone at a safe boundary and verifies the destination, branch, HEAD, and status in a later call.
If synchronization or movement is blocked, the agent remains in the implementation worktree and reports the blocker.

## Risks / Trade-offs

- [A required integration check depends on external CLI packages and browser availability] -> Pin versions, install only development tools, run the same commands locally, and keep the two jobs separate for clear failures.
- [Enabling protection before successful check contexts exist can block merging] -> Verify both jobs on the bootstrap pull request and only then configure required contexts.
- [Administrator enforcement can make a broken workflow difficult to repair] -> Keep authenticated repository administration available to update protection deliberately, never bypass it silently, and repair the workflow through a pull request whenever checks can still run.
- [Every edit requiring a worktree adds ceremony] -> Use Worktrunk's deterministic creation flow and retain the stronger isolation rather than defining subjective exceptions.
- [Hosted Linux behavior can differ from local macOS behavior] -> Preserve local verification as delivery evidence and do not claim platform behavior that neither environment executed.

## Migration Plan

1. Add the policy, workflow, and development documentation in a dedicated Worktrunk branch.
2. Run the complete local gate and Hardlook review.
3. Push the branch and open the bootstrap pull request while `master` remains unprotected.
4. Verify both hosted checks on the pull request.
5. Apply classic branch protection with the verified check contexts and read the settings back through the GitHub API.
6. Merge through GitHub under the new protection.
7. Fast-forward the primary checkout, verify the merge, and return the coding-harness session.

If the hosted workflow cannot run reliably, do not enable the new required contexts.
If protection has already been enabled with unusable contexts, remove only the blocking required contexts through authenticated repository administration, repair the workflow, and reapply the intended settings after a successful run.
