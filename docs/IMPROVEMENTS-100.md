# 100 improvement candidates

Repository-specific backlog, ordered by group priority. Done means implemented in this change. Other items are proposals, not installed tools or completed features. Tool candidates need compatibility and license review. Skill candidates are reusable workflow ideas.

## Trust and releases

1. Done: Free Microsoft detection-review link in App Self-Check.
2. Done: Calculate release-note checksum after optional signing.
3. Done: Browser-download option for manual updates.
4. Done: Refuse automatic updates without a published checksum.
5. Detection worksheet: capture exact threat name, hash and security intelligence version.
6. Deliberate stable releases: avoid publishing a new binary on every main push.
7. Build provenance: publish source commit, compiler version and build inputs.
8. Artifact attestation: evaluate GitHub provenance verification for releases.
9. Defender CI scan: retain final-artifact scan results without claiming safety.
10. Done: Update rollback restores the original pathname if replacement or process launch fails.

## Windows and background work

11. Done: Close, minimize or cancel choice when closing the window.
12. Done: Warn about active tasks before exiting.
13. Done: Remember close preference with Ask, Minimize and Close in settings; active tasks still require exit confirmation.
14. Optional tray mode: Restore and Exit through a tray icon.
15. Quiet minimized mode: reduce animation and polling.
16. Completion toast: opt-in notification with build result and output path.
17. Persist window placement: validate bounds against available monitors.
18. Sleep prevention: opt-in execution-state request while builds run.
19. Task ownership: offer wait, detach or cancel for tracked child processes on exit.
20. Single-instance handoff: bring the existing project window forward.

## Debugging and logs

21. Done: Add twelve diagnostic signatures to the existing six.
22. Done: Case-insensitive matching with one diagnosis per signature.
23. Done: Open log/txt files with an 8 MiB import limit.
24. Done: Copy a diagnostic report with version, matches and next steps.
25. Done: Include backtraces in app panic logs.
26. Done: Explain that no known pattern does not mean success.
27. Done: Show numbered surrounding lines for each match and sort diagnoses by their position in the log.
28. Severity filters: toggle errors, warnings and informational output.
29. Search navigation: jump between matches and show counts.
30. Partial: Copied/saved reports can redact known local paths. Automatic token detection and a dedicated preview remain proposed.

## Build reliability

31. Dry-run preview: show the executable and arguments before packaging.
32. Named presets: save platform and configuration combinations.
33. Build queue: sequence projects without cook contention.
34. Cancellation escalation: graceful stop before timed process-tree termination.
35. Heartbeat: distinguish quiet output from an unresponsive tool.
36. First-cause capture: preserve actionable errors before summary failures.
37. Environment snapshot: record engine, SDK, compiler and target per build.
38. Disk estimate: use prior builds to predict required space.
39. Archive verification: check zip readability before marking success.
40. Collision protection: prevent builds writing to the same output directory.

## Unreal tools

41. Blueprint validation scopes: check selected content folders.
42. Redirector audit: list candidates before running fix-up.
43. Plugin target matrix: compare enabled plugins with target platforms.
44. Map reference audit: find missing packaging and default maps.
45. Asset-size comparison: identify growth between packages.
46. Warning baseline: highlight new cook warnings.
47. Config diff: preview INI changes before writing.
48. Engine switch history: record association changes and restore paths.
49. DDC overview: show cache size and cleanup candidates.
50. Crash grouping: normalize call stacks for local triage.

## Deployment and devices

51. Device selector: label Android devices by model and serial suffix.
52. Logcat capture: scope device logs to the launched game process.
53. SDK compatibility: compare installed versions with engine requirements.
54. Deployment preview: show destination, size and overwrite scope.
55. Upload retries: handle transient failures with cancellation.
56. Build hash manifest: record hashes for distributable outputs.
57. Release-note editor: review generated git notes before export.
58. Build receipt: export target, configuration, time and artifact hashes.
59. Launch profiles: save editor and game argument combinations.
60. Pak inspection: integrate the selected engine's inspection tool.

## Quality and development tools

61. Done: Run cargo test before publishing a release.
62. cargo fmt gate: establish formatting in a separate baseline change.
63. cargo clippy gate: baseline warnings before enforcing new ones.
64. cargo audit candidate: review dependency advisories in CI.
65. cargo deny candidate: evaluate license and dependency checks.
66. Dependency update policy: group changes and require test evidence.
67. Partial: Missing-checksum, failed-install, failed-launch and successful-backup tests are implemented. Bad-hash and locked-file fixtures remain proposed.
68. Process fixtures: test cancellation and child ownership.
69. Log corpus: retain sanitized failures across engine versions.
70. Partial: Live Windows close/cancel/minimize/restore and saved-preference smoke tests completed; live build cancellation remains proposed.

## Performance and usability

71. Bounded log memory: retain recent lines while streaming full logs to disk.
72. Done: Background import keeps disk reading and encoding conversion off the UI thread and reports read failures.
73. Done: Decode UTF-16 LE/BE and UTF-8 BOM logs, rejecting invalid encoding with an explanation.
74. Keyboard dialogs: focus handling, Escape and clear default actions.
75. Accessible status: pair color with readable labels.
76. DPI checks: verify layouts at 100, 150 and 200 percent scaling.
77. Reduced motion: disable decorative animation while preserving progress.
78. Project favorites: show engine and last-build state.
79. Log bookmarks: persist selected lines for later inspection.
80. Local help search: find app guidance while offline.

## Reusable skill proposals

81. Failure triage skill: identify the first cause from minimal build logs.
82. Release verification skill: compare final artifact and published hash.
83. Defender investigation skill: distinguish detections, PUA and reputation warnings.
84. SDK setup skill: verify supported versions before changing installations.
85. Plugin migration skill: prepare compatibility checks, backup and diff.
86. Packaging regression skill: compare receipts and locate time or size growth.
87. Android triage skill: inspect device state, install output and application logs.
88. Report privacy skill: preview sensitive-data redaction before sharing.
89. Windows smoke skill: exercise window controls, scaling and keyboard access.
90. Cleanup skill: identify regenerable folders and preview deletion scope.

## Support and settings

91. Support bundle preview: select logs and metadata before local zip export.
92. Config schema versioning: migrate settings with backup and parse recovery.
93. Settings export/import: preview changes and exclude credentials.
94. Offline mode: suppress update and connectivity checks.
95. Network timeouts: distinguish offline, timeout and server failures.
96. Health-check history: retain recent failures and resolution timestamps.
97. Self-check report: copy version and check results without project contents.
98. Troubleshooting index: link diagnoses to app actions and official documentation.
99. Local usage counters: opt-in prioritization data without network telemetry.
100. Backup browser: list app-created backups, preview differences and restore.

## Evidence and limits

[Microsoft's developer FAQ](https://learn.microsoft.com/en-us/defender-xdr/developer-faq) covers detection review. [SmartScreen reputation guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation) describes the separate reputation system. No change guarantees Defender acceptance. An exact detection and final artifact are needed for investigation. No paid purchase, external submission or security exclusion is performed by these changes.
