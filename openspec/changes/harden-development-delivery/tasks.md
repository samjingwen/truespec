# Tasks

## 1. Define repository delivery policy

- [x] 1.1 Replace the abbreviated `AGENTS.md` delivery section with the agreed Worktrunk, implementation, verification, Hardlook, pull-request, merge, synchronization, session-return, and reporting sequence; define a harness-neutral transition contract, add OpenCode's verified `tools.opencode.session.move` mapping, state the evidence required for future harness mappings, and verify every repository edit and authority boundary has an explicit rule.
- [x] 1.2 Update the public development guide with the required local and hosted checks and pull-request workflow; verify its commands and check names match `AGENTS.md` and the workflow.

## 2. Add hosted verification

- [x] 2.1 Add a read-only `.github/workflows/ci.yml` for pull requests and pushes to `master` with required jobs named `Quality` and `Browser and OpenCode`; verify actions and CI-only tools are pinned and the commands match the documented gates.
- [x] 2.2 Run the complete ordinary, browser, and OpenCode gates locally; verify all commands pass without adding TrueSpec runtime dependencies.

## 3. Review and protect delivery

- [x] 3.1 Review the complete base-to-head diff with Hardlook, address every accepted finding, rerun invalidated checks, and record any remaining limitation.
- [ ] 3.2 Push the branch and open the bootstrap pull request; verify both required GitHub checks pass for the exact reviewed head.
- [ ] 3.3 Apply classic `master` protection after both contexts exist and verify through the GitHub API that pull requests, strict checks, administrator enforcement, conversation resolution, force-push prevention, and deletion prevention match the approved design.

## Workflow follow-up

- Merge the bootstrap pull request through GitHub after all required checks and conversations are complete.
- Verify the exact merge SHA and fast-forward the recorded primary checkout without stashing, resetting, cleaning, rebasing, or forcing.
- Return the coding-harness session to the primary checkout when supported and verify its path, branch, HEAD, and status.
- Archive the completed OpenSpec change in a later pull request.
