# Kaizen backlog

## Current readiness increment (source/test verified)
- Readiness scans run in one background worker; results require the current project path, target and generation, preventing stale A-to-B-to-A completion from replacing newer data.
- `.uproject` reads are limited to 4 MiB. Unreadable/oversized descriptors report Unknown and give a next step. The dashboard shows active scanning.
- Tests cover unreadable and oversized descriptors, path/target mismatch, and generation changes. Existing handoff tests verify existing notes are preserved on overwrite attempts.
- Remaining: cancellation/deadline, build gating on a fresh scan, structured evidence/action fields, and live Windows UI acceptance.
- Verified build: `C:\\Users\\Public\\Repo\\UnrealDevtool\\target\\build-verify\\debug\\unreal_devtool.exe`.

Evidence comes from the read-only product audit and local implementation in this checkout. Status describes source and automated checks only; a feature is not runtime-verified unless noted.

## P0 — Protect the user's work

### Stop packaging from closing unrelated Unreal processes — DONE, SOURCE-VERIFIED
- Evidence: `src/ops/package.rs`; old global `taskkill /im` behavior replaced by a preflight that asks the user to close Unreal themselves.
- User benefit: an open editor for another project cannot be silently closed by packaging.
- Acceptance: packaging returns an actionable preflight result whenever an Unreal editor/commandlet process exists; no editor process is killed. Regression test checks the user-facing block message. Runtime packaging was intentionally not run.

### Make local Git actions explicit — DONE, SOURCE-VERIFIED
- Evidence: `src/ops/git.rs`, `src/ui/git.rs`, `src/ui/rail.rs`.
- User benefit: no hidden stage-all, no push as part of branch update, and no automatic force-with-lease push.
- Behavior: Commit & Push uses only staged paths and visibly labels the remote push. Update branch from main requires its confirmation panel, fetches and rebases locally, and explicitly reports that it did not push.
- Acceptance: empty index refuses commit; unsaved files are not staged; update flow invokes no push. Unit test covers empty/nonempty staged path parsing; source inspection confirms the sequence. No app Git flow was run against the user's repository.

### Isolate duplicate project names – DONE, TESTED
- Evidence: `src/config.rs`.
- User benefit: same-named projects receive distinct build settings, UAT options, size budgets and playtest files.
- Behavior: settings key includes a normalized full-path SHA-256; case-only path changes match on Windows. Legacy stem-only settings migrate to the first opened matching project. If migration rename fails, the app fails closed to project defaults and leaves legacy data untouched, preventing another same-name project from reading it.
- Acceptance: same basename in different directories yields different keys; case differences yield the same key; a blocked migration does not fall back to the shared legacy file. Automated tests cover these cases.

## P1 — Give creative teams useful task flows

### Save repeatable playtest presets and a launch receipt – IMPLEMENTED, UNIT/BUILD VERIFIED; UI/UNREAL UNVERIFIED
- Evidence: `src/config.rs`, `src/ui/tools.rs`, `README.md`, `docs/REFERENCE.md`.
- User benefit: designers/QA can save launch flags, reload them per project, and copy a receipt with executable, arguments, timestamp and expected project log path.
- Acceptance: presets round-trip as local JSON; launch success records Windows accepting the request, while spawn failure records the OS error; neither claims game completion. Serialization test passes. No editor/game launch or visual smoke test was performed.

### Make navigation describe work goals — PARTIAL
- Evidence: `src/ui/shell.rs` and `src/ui/tools.rs`.
- User benefit: tooltips say check readiness, playtest, validate content and inspect size, instead of requiring Unreal command vocabulary.
- Remaining acceptance: on a clean install, a designer and QA tester can each reach check/playtest/report in under one minute without help. Requires UI smoke/usability validation.

## P2 — Improve result quality and product confidence

### Replace string-inferred packaging outcomes – DONE FOR PACKAGING; OTHER TASKS REMAIN
- Evidence: `src/ops/package.rs`, `src/types.rs`, `src/app.rs`, `src/ui/mod.rs`, `src/ui/run.rs`.
- User benefit: packaging completion carries explicit success/failure/cancellation and separate ZIP creation/verification states. The Done screen and build history use those values, never parse the status text.
- Acceptance: a misleading `[DONE]` status cannot turn a failed result successful; cancellation remains distinct; a requested ZIP must be verified, while a workflow that did not request a ZIP can succeed with its output folder. Unit tests cover requested/no-archive, failed archive, cancellation, and stale-result reset; archive integrity tests cover a valid ZIP and rejected data.
- Remaining: generic non-packaging background tasks still return human-readable strings; converting those is a separate change.

### Offline mode for safe local inspection – IMPLEMENTED
- Evidence: `src/config.rs`, `src/app.rs`, `src/ui/mod.rs`, `src/webview.rs`, `src/ui/browser.rs`, `src/ops/mod.rs`, `README.md`, `docs/REFERENCE.md`.
- User benefit: `UDT_OFFLINE=1` starts without update/GitHub checks, local LLM discovery/chat, embedded web pages, external links, cloud upload/rclone setup, Android device discovery/install, or Git fetch/push/merge-push. Local inspection, packaging/playtesting, and local report export remain available. The environment switch leaves saved settings unchanged.
- Acceptance: offline mode parser tests pass. The debug app remained alive for 5 seconds with an isolated `%APPDATA%` fixture and this switch. Native UI interaction is unavailable in this environment.

### Draft a local issue report from a run or launch receipt — IMPLEMENTED, UI UNVERIFIED
- Evidence: `src/ops/issue_report.rs`, `src/ui/tools.rs`, `README.md`, `docs/REFERENCE.md`.
- User benefit: QA and non-programmers can edit expected/actual behavior and reproduction steps beside a build or playtest receipt, preview Markdown, and export it to a chosen local path.
- Privacy: project identity is a project name; launch arguments and log contents are never copied. Log paths remain unchecked until explicitly selected. Missing status/behavior data is labeled rather than inferred.
- Acceptance: serialization, no false runtime confirmation, explicit attachment selection, Markdown export and export errors have unit coverage. Native screen review remains pending.

### Hand off a verified build to QA and stakeholders — IMPLEMENTED, UI/UNREAL UNVERIFIED
- Evidence: `src/ops/handoff.rs`, `src/ops/package.rs`, `src/ui/run.rs`.
- User benefit: successful builds include a portable note with target/configuration/method, archive size and SHA-256, plus a tester checklist. QA can identify a transferred artifact and record a retest without learning UAT terminology.
- Privacy and scope: includes project basename and archive basename only; no local file paths, logs, launch arguments or Git remote details. Created only after the existing ZIP integrity verifier passes. Explicitly says runtime/gameplay are not verified.
- Acceptance: tests cover exact-file hashing, report scope/wording, Markdown escaping and create-new overwrite protection. Native UI copy behavior and a real Unreal package still require a manual smoke test.

### Harden build artifacts and provenance – PARTIAL
- Evidence: backlog candidates 31, 35, 37, 39 and 40 in `docs/IMPROVEMENTS-100.md`.
- User benefit: preview command, prevent output collisions, detect quiet/hung tools, capture engine/toolchain/commit at task start, validate archive integrity.
- Acceptance: deterministic command preview; collisions blocked; logs show last heartbeat; receipt records immutable task-start identity; corrupt ZIP is never offered as verified. Archive integrity verification is implemented and tested; other items remain.

### Qualify product distribution and supported combinations — NOT STARTED
- Evidence: `README.md` documents unsigned Windows executable and limited real Android/Linux/device testing; `.github/workflows/release.yml` publishes on each push to main.
- User benefit: fewer surprise security prompts and clearer claims about what works.
- Acceptance: stable/manual release gate with provenance and regression evidence; tested engine/platform matrix; review third-party asset redistribution before changing bundled assets or license claims.

## Verified this increment
- Previous increment: `cargo test --locked`: 99 passed, 0 failed, 2 ignored.
- Current increment: `cargo test --locked` passed (121 passed, 0 failed, 2 ignored); `cargo build --locked` passed; `git diff --check` passed with CRLF/LF advisory warnings. Handoff tests verify the SHA-256 of known bytes, streamed byte count, no local path disclosure, honest tester wording, Markdown escaping and create-new overwrite protection. UI copy behavior and a real Unreal build remain unverified.
- `cargo fmt --check` was blocked in the previous increment because Windows Application Control blocked `cargo-fmt` (OS error 4551); this increment will not retry it.
- 55 existing `f32` fallback warnings remain. Debug binary: `C:\Users\Public\Repo\UnrealDevtool\target\debug\unreal_devtool.exe`.
