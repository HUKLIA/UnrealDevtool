# Project Context: Unreal DevTool first release improvement

## Scope
Local-only work in `C:\Users\Public\Repo\UnrealDevtool`. No cloud access, telemetry, paid dependencies, public release, Git operations against this project, Unreal packaging, or editor-closing launch. Initial `git status --short` was clean. No applicable `AGENTS.md` found in workspace/ancestors.

## Acceptance criteria
- Packaging never closes or force-kills an editor process; ask the user to close Unreal.
- Git commit does not stage all files; branch update does not push or force-push.
- Same-named projects keep independent local preferences with a legacy read/migration path.
- Include local playtest presets and a useful launch report.
- Improve goal-based guidance for non-programmers.
- Add regression tests and run local tests/build. Avoid unverified claims.

## Progress
- [x] Run project readiness checks in a single background worker, apply only results matching the current project, target, and scan generation, and show a scanning state. Project descriptor reads over 4 MiB or unreadable files are reported as Unknown with next-step guidance.
- [x] Add readiness regression checks for oversized/unreadable descriptors and mismatched path/target plus A-to-B-to-A generation changes.
- [x] Inspect instructions and initial repository status.
- [x] Remove process-name-based editor termination; preflight asks the user to close Unreal.
- [x] Commit only already staged Git files; branch update fetches/rebases locally and does not push.
- [x] Hash normalized full project paths for per-project settings; migrate legacy stem configs and fail closed if migration cannot complete.
- [x] Add local per-project playtest presets and a persistent receipt that distinguishes spawn acceptance from launch failure.
- [x] Verify every packaged ZIP entry before making the archive available as the build result.
- [x] Return typed package completion through the operation and Done UI; require ZIP verification only when a ZIP was requested, and expose only verified archives for shipping. Successful folder-only work opens its output folder.
- [x] Persist typed terminal outcomes in build history, including cancellation; older `build-info.txt` records still parse and use their prior success/failure summary.
- [x] Add editable local issue-report drafts from current build, saved build history, or launch receipt. Include requested/confirmed status, expected/actual behavior, reproduction steps, and only explicitly selected log paths; preview and export Markdown locally without including log contents or launch arguments.
- [x] Add `UDT_OFFLINE=1` to suppress network-backed startup checks and disable app network surfaces without rewriting saved settings.
- [x] Improve playtest/readiness navigation language and update user docs.
- [x] Add regression tests and run local tests/build.
- [x] Start the debug app with `UDT_OFFLINE=1` and isolated `%APPDATA%`/project fixtures; it remained alive for 5 seconds before the smoke harness stopped its own process. No Unreal operation was started.
- [x] Add portable `build-handoff.md` beside a successfully verified ZIP, with archive SHA-256/size, target/configuration/build method and a tester checklist; add a successful-build copy action.

## Blockers / unknowns
- UI interactions and persistence across restart still need a Windows smoke test. In this increment CUA reported no apps or browsers, did not expose `listWindows`, and the offline demo process exited before a window could be inspected; visual copy-action validation remains blocked.
- Playtest receipt records only a launch request, not actual game completion; issue reports preserve that as unconfirmed and let the user add observed behavior and QA notes.
- Non-packaging background tasks still use string results; package completion now uses typed outcomes end to end.
- Legacy same-stem settings cannot identify their original project; first project opened claims the old file. If migration is blocked, the app uses project defaults rather than risk cross-project leakage.
- ZIP integrity is checked before it is surfaced; archive requested/created/verified states are separate from the package terminal outcome. Package ZIP output is currently always requested by the existing UI flow; folder-only success is covered at the result-contract level for optional archive workflows.
- Build history now distinguishes cancellation from failure for newly written records and remains backward-compatible with existing records.
- Offline mode blocks DevTool-controlled probes, WebViews/external links, cloud upload/rclone setup, Android device discovery/install, update install, and Git fetch/push/merge-push. Local Git inspection, package/playtest actions, and local report export remain available. It does not firewall network activity inside Unreal/UAT/game child processes.
- 55 pre-existing compiler warnings remain.

## Verified results
- Final bounded pass: `cargo test --locked` passed (124 passed, 0 failed, 2 ignored) with `TEMP`/`TMP` set to `target\\test-tmp`; `cargo build --locked --target-dir target\\build-verify` passed. Verified executable: `C:\\Users\\Public\\Repo\\UnrealDevtool\\target\\build-verify\\debug\\unreal_devtool.exe`. `git diff --check` passed (line-ending advisories only).
- Default `cargo build --locked` could not replace `target\\debug\\unreal_devtool.exe` (Windows returned Access is denied). This is recorded as a file replacement/access failure; its cause was not verified. No alternate-path retry was made beyond the distinct verification target above.
- The readiness worker is single-flight and generation checked, but has no cancellation/deadline, does not gate packaging, and does not yet attach structured evidence/action fields to each result. Manual UI validation remains outstanding.
- Previous increment: `cargo test --locked`: 99 passed, 0 failed, 2 ignored.
- Current handoff increment: `cargo test --locked` passed (121 passed, 0 failed, 2 ignored); `cargo build --locked` passed; `git diff --check` passed with CRLF/LF advisory warnings. Build emitted 55 pre-existing `f32` fallback warnings.
- The isolated offline demo was launched to inspect the finished-build screen, but CUA returned empty app/browser inventories and the process exited before visual inspection. No Unreal project was opened or built.
- `cargo fmt --check` was blocked in the previous increment because Windows Application Control denied `cargo-fmt` (OS error 4551); do not retry it through another route.
- No Unreal packaging or project Git action was run.

## Next useful increment
- When native Windows UI access is available, use `C:\Users\Public\Repo\UnrealDevtool\target\debug\unreal_devtool.exe` with `UDT_OFFLINE=1` and an isolated `%APPDATA%`/fixture project. Check: (1) save/load a playtest preset, confirm overwrite asks for a second click, and verify it remains project-specific after switching projects; (2) create reports from a launch receipt and saved build history, edit fields, verify logs start unchecked, preview and export to a new local `.md`; (3) confirm a second export cannot overwrite the first file; (4) cancel a test background task and confirm the Done/history states say cancelled; (5) open build history after success/failure/cancellation; (6) inspect empty states for no project, no build, and no receipt. Native UI interaction is blocked in this agent environment; Unreal build/runtime verification is also pending and must use a disposable test project.
- Prioritize that manual UI checklist, then a disposable Unreal validation of package cancellation, existing-version collision protection, and archive/no-archive reporting. Defer additional feature work until those checks pass.
- For handoff validation, package a disposable Unreal project and confirm `build-handoff.md` appears only beside a verified ZIP, its recorded SHA-256 matches the ZIP after copying, and **Copy test handoff** puts the same Markdown on the clipboard. Also verify failed/cancelled builds do not get a handoff note.
