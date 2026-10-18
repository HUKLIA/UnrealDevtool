# Editor won't open: diagnose and repair

Open **Unreal tools > Editor won't open**, use the first-run recovery button, or search **Editor won't open** with Ctrl+K. Windows association diagnosis works even before choosing a project.

This workflow was motivated by a case where choosing an engine did not solve “Invalid command line,” Unreal right-click options disappeared, and Windows reported that no application was associated with `.uproject`. Those symptoms should not be treated as proof that the project is corrupt.

## Diagnose before changing anything

The scan checks the actual editor executable, original UnrealVersionSelector location, Windows open command and default-app override. With a project selected, it validates the descriptor, compares its requested engine with the selected engine, checks for missing C++ source, and identifies configured startup maps and DX12 rendering.

Startup-map and DX12 notices are troubleshooting candidates, not proof of a crash cause. The scan does not claim that a project is compatible merely because an engine is installed.

Engine discovery includes Epic Launcher's `LauncherInstalled.dat`, Launcher registry installations, custom aliases/GUIDs and native source-engine project layouts. Empty `EngineAssociation` is meaningful for a native project and is not automatically replaced.

## 1. Direct-open test

Preview a direct editor launch or a temporary `-dx11` launch. The preview shows the executable and separate arguments. Confirming starts that executable directly with the selected project and `-log`; Windows file associations are bypassed.

If direct opening succeeds while double-click fails, investigate the Windows association. If the editor starts and then crashes, investigate logs, modules, plugins or startup settings instead. Starting a process is not confirmation that the project loaded.

Choose the intended engine before launching. Opening with a different engine can convert project files/assets; use a copy or commit first. DX11 can disable rendering features and does not resolve every GPU issue. The app does not launch Unreal automatically during a scan.

## 2. Windows project association repair

Locate the original `UnrealVersionSelector.exe` when discovery cannot find it. Do not copy it into another folder. No selector or replacement executable is downloaded or bundled.

**Preview Windows repair** lists the exact values under `HKCU\Software\Classes` that will change. **Confirm action** registers the `UnrealDevTool.ProjectFile` handler, editor opening, project-file generation, engine switching, icon and OpenWith registration. `.uproject` paths are quoted. Existing machine-wide `Unreal.ProjectFile` settings are not edited.

A JSON backup of the previous per-user values is written and flushed to `%APPDATA%\UnrealDevTool\repairs\association-<timestamp>.json` before mutation. Changed values cause the operation to refuse; partial failures attempt rollback. Explorer is notified after successful apply/restore.

Windows can prioritize a protected `UserChoice` default over extension registration. The workflow detects it and provides a **Windows Default apps** button for the user to select the Unreal project handler. It does not edit UserChoice or its hash, request elevation, or promise that registration alone overrides a Windows default.

Undo is available in-session or through **Restore Windows repair from backup** after restarting. The restore preview lists the values. Unexpected registry paths, changed backup files and changed current values are refused. Empty registry keys may remain after restoring absent values.

## 3. Targeted project settings

Every setting action first produces a before/after preview:

- **Use selected engine** changes only the descriptor's engine association, using a registered identifier that resolves to that installation. Unregistered installs are refused instead of guessing from a folder name.
- **Clear editor startup map** clears `EditorStartupMap`, including duplicate scalar entries. `GameDefaultMap` is preserved.
- **Set Windows renderer to DX11** changes only `DefaultGraphicsRHI` in the Windows target-platform section. Prefer the temporary launch test first.
- **Reset generated editor preferences** resets one existing project-local `EditorPerProjectUserSettings.ini` under `Saved/Config/WindowsEditor` or `Windows`. It does not remove all of Saved, autosaves, assets or project Config.

Close Unreal Editor/commandlets before applying or restoring settings. The app rechecks for running editor processes immediately before writing. Each repair saves a unique adjacent `.devtool-repair-<timestamp>.bak`; older backups are retained. The exact file contents must still match the preview, and linked targets outside the project are refused. Non-UTF-8 settings are refused rather than rewritten with guessed encoding.

**Undo last settings repair** restores exact prior bytes in the same app session and project. After restarting, adjacent backups remain available for manual recovery. Automatic undo refuses files changed by the editor or subsequent work, and refuses modified backups.

Invalid descriptors, absent source, missing plugins, corrupt assets and unsupported engine conversions cannot all be fixed by changing settings. The workflow does not disable every plugin, delete generated directories, modify engine installation files, or declare success from a process spawn alone.

## Verification

Regression tests cover quoted project paths, missing selectors and malformed open commands, registry apply/restore and rollback with an isolated store, scope restrictions, stale previews/backups, startup-map preservation, duplicate scalar keys, DX11 edits, preference reset, unchanged autosaves, exact file undo, malformed encoding, unique backups and native engine discovery.

Live Windows smoke checks verified the first-run entry point, recovery navigation, original selector discovery in a nonstandard installation, selected-project engine matching, and startup-map/DX12 notices against an isolated project fixture. No Windows association repair was applied to the development machine and no real user project was converted or repaired during verification.

## Sources

- [Epic support's explanation of UnrealVersionSelector and engine discovery](https://forums.unrealengine.com/t/engine-association-issue/2651662/2)
- [Epic command-line arguments](https://dev.epicgames.com/documentation/en-us/unreal-engine/command-line-arguments-in-unreal-engine)
- [Epic command-line reference, including DX11](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-command-line-arguments-reference)
- [Microsoft Windows file types](https://learn.microsoft.com/en-us/windows/win32/shell/fa-file-types)
