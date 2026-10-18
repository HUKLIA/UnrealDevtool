# Feature verification — 2026-10-18

## Implemented from the backlog

- Saved Ask / Minimize / Close behavior, with a remember-choice checkbox and Settings control. Active work always prompts before exiting.
- Close dialog above sheets, a click-blocking backdrop, and suppression of embedded webviews while the dialog is visible.
- Background log import with an actual 8 MiB read limit, UTF-8 BOM and UTF-16 LE/BE handling, and readable encoding errors.
- Chronological diagnostic matches, numbered context, and context included in reports.
- Copy and save reports with optional redaction of known project, engine and user-directory roots. This is not general-purpose secret detection.
- Updater rollback for file replacement and process-launch failures, retaining the staged download when launch fails. A successful process launch followed by a crash is outside this recovery mechanism.
- Earlier changes in the same commit include expanded diagnostics, panic backtraces, checksum enforcement, manual-download choice and release hash correction.

## Automated checks

`cargo test --locked`: 77 passed, 0 failed, 2 ignored. Coverage added for close policies during idle and active work, preference serialization, diagnostic ordering/context, UTF-16/UTF-8 decoding, malformed encodings, oversized files, redaction, checksum refusal and update rollback.

The ignored tests require real Unreal tools. No Unreal project was packaged during these checks. Existing compiler warnings remain outside this change.

## Live Windows checks

Using the computer-use skill against the newly compiled debug app:

1. Title-bar Close opens the choice dialog and keeps the app running.
2. Cancel dismisses the dialog and keeps the app running.
3. Alt+F4 opens the same choice dialog.
4. Minimize keeps the process/window alive; the Windows automation runtime confirms the window is minimized.
5. Restoring the window returns to the app with the choice dialog dismissed.
6. Remember Minimize writes the preference; the next Alt+F4 minimizes directly.
7. Settings exposes the saved preference and can reset it to Ask.
8. Close app removes the app window.
9. After the overlay fix, the dialog remains visible and usable above Settings; Minimize still works.

Active-work exit policy is covered by automated tests rather than a live packaging run. The UI checks do not establish Defender acceptance.
