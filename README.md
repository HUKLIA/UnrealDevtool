# Unreal DevTool

A Windows desktop tool for Unreal Engine 5 developers: packages builds, regenerates Visual Studio project files, manages Git, and includes a few extras — all from one GUI.

> **Study & research project. Not for production.**
> Feel free to use it for testing or as a reference for your own work.

---

## UI

A brief animated boot-log splash (built from what was actually detected — your real project/engine/git state, not placeholder text) leads into the main window: a dark "glass panel" theme with a teal accent, a faint background tech-grid, and a soft corner glow. The window opened wide (~1040×760) for a multi-column "bento grid" desktop layout — Dashboard and Package show side-by-side cards, Chat and Extras use a left sidebar + main content area — organized into five tabs instead of one long scrolling button list. Tab content fades in on switch. The window centers itself on the monitor once at launch, and is otherwise completely free to move/resize — it never repositions itself again after that first frame.

| Tab | Contents |
|---|---|
| **Dashboard** | Two-row bento grid: project path + engine path side by side (with **Browse…** overrides), **Rebuild VS Files**, then preflight diagnostics + a build-log scanner side by side (engine/project validity, disk space, the space-in-path UAT bug, a scan of the most recent build log against known UAT/UBT error signatures, and a box to paste an arbitrary log excerpt to scan instead) |
| **Package** | Left: package/exe name, version (auto-incremented or custom), the space-fix warning. Right: a custom-painted circular progress ring (0% idle / holds at 100% after a run — the actual *live* packaging progress still uses the full-screen Miku view, unchanged) plus **Start Packaging** / **Fast Package** |
| **Git** | Commit & push, sync with main (fetch → rebase → push, fully automatic), merge current branch into main |
| **Chat** | Dev Assistant — left sidebar shows detected LLM servers as selectable cards (auto-detects Ollama `:11434` and LM Studio `:1234`) plus a live "context injected" preview (project/engine/space-warning/git branch); right side is the chat itself, streaming responses with that same context sent on every message |
| **Extras** | Left sidebar: Miku Visualizer (2D GIF / 3D WebGL toggle), Mini-Games (Cookie Clicker, Sponder Bird), **App Self-Check** (the DevTool's own install/config/update health — separate from Dashboard's project/engine diagnostics), DM on Discord, Customize (GIF/sound overrides, accent color — five presets or a full picker), and **Quick Links** underneath |

Engine detection reads `EngineAssociation` from the `.uproject` file to auto-find the exact matching engine via the registry. If auto-detection can't find it (non-standard install, source build, missing launcher registry keys), Browse… lets you point at the engine folder manually — the override persists and always wins over auto-detection until cleared.

---

## Quick Links

Fully user-editable — click **✏ Edit** under Quick Links (Extras tab) to rename, retarget, add, or remove any of them; changes save immediately to `links.json`. Seeded by default with Claude, ChatGPT, Gemini, Epic Games, and the Unreal docs assistant (real URLs), plus Trello, Jira, Task List, and Requirement Check (empty URL — there's no universal default for a team's own board/doc, so these start unset). Clicking a link with no URL set opens the editor instead of navigating nowhere.

---

## The space-in-path bug (and its fix)

Unreal's own UAT/UBT batch scripts have a long-standing bug with spaces in paths — most commonly hit via the *default* Epic Games Launcher install location (`C:\Program Files\Epic Games\UE_5.x`), or a project folder with a space in its name. It shows up as a cryptic `'C:\Program' is not recognized as an internal or external command` failure, often after a long build.

Check PC Setup (and the config panels for Package/Rebuild VS Files) detect this and offer a one-click **Fix automatically**: it creates an NTFS directory junction aliasing the affected folder(s) to a space-free path (`C:\UEDevToolLink\...` or `%ProgramData%\UEDevToolLink\...`) and routes UAT/UBT invocations through that instead. Nothing is moved or copied — the junction is just an alternate, space-free path to the same folder. Once applied, it's used for both packaging and VS Rebuild for the rest of the session.

---

## Package versions

Versions auto-increment as `v0.0.1`, `v0.0.2`, … based on existing build folders. You can also enter a custom version before packaging. The version string is validated — it cannot be empty or contain characters that are illegal in Windows file names (`\ / : * ? " < > |`).

---

## Sync with main

"Sync with main" is fully automatic:
1. `git fetch origin main`
2. `git rebase origin/main`
3. `git push --force-with-lease origin <current-branch>` (or a regular push if already on main/master)

No manual pull or push needed after clicking the button.

---

## Auto update check

The app checks GitHub for a newer release on startup and then every 5 minutes while running. If a new version is found, a banner appears immediately. No restart needed to see the update prompt.

Installing an update renames the running exe aside and drops the new one in its place (Windows allows renaming a running executable — it only blocks deleting one without `FILE_SHARE_DELETE`), then relaunches. Both that rename and the later cleanup of the old exe retry with backoff, since antivirus real-time scanning commonly grabs a freshly-written `.exe` for a moment right after it's closed. If the app is installed somewhere without write access (e.g. under `C:\Program Files\...` without admin rights), the update fails fast with a clear message instead of a cryptic OS error — App Self-Check surfaces this too.

---

## Project layout

```
src/
  main.rs         entry point, window setup
  app.rs          DevToolApp state + all non-UI action methods
  ui/             egui panels (one module per feature area)
    mod.rs          tab bar + routing, project/engine path rows, media/DM panels
    intro.rs         boot-log splash screen
    dashboard.rs      Dashboard tab (project/engine rows + inline diagnostics)
    package.rs        Package tab config panel, upload panel, post-package prompts
    vs.rs             VS-rebuild config panel (opened from the Dashboard tab)
    git.rs            Git tab panels
    chat.rs           Chat tab (Dev Assistant)
    extras.rs         Extras tab sidebar nav + Quick Links (editable) + Miku/Games sub-panels
    circular_meter.rs Custom-painted progress ring (Package tab)
    preflight.rs      Check PC Setup content, space-fix warning box
    selfcheck.rs      App Self-Check panel (an Extras sub-tab)
  ops/            everything that isn't UI — file/process/network work
    package.rs       UAT BuildCookRun, zip, upload
    vs.rs            GenerateProjectFiles.bat / Build.bat
    git.rs           git plumbing
    preflight.rs      space-in-path fix, disk space, PC-setup checks
    diagnostics.rs    known-error signature table, build-log scanner
    llm.rs            Ollama / LM Studio client (provider detection, streaming chat)
    selfcheck.rs      app-itself diagnostics
    update.rs         GitHub release check, self-update, old-binary cleanup
    discord.rs        DM-on-Discord
  engine.rs       Unreal Engine detection (registry / EngineAssociation)
  config.rs       all persisted settings (%APPDATA%\UnrealDevTool)
  types.rs        shared enums (GitState, IdeChoice, ...)
  theme.rs        colors, accent color persistence
  audio.rs, gif.rs, webview.rs   media playback, embedded WebView2 panels
```

---

## Build

**Debug** — run directly on Windows:
```powershell
cargo run
```

**Release** — build from WSL2:
```bash
sudo mkdir -p /mnt/q && sudo mount -t drvfs Q: /mnt/q
cd /mnt/q/Rust/DevTool
bash build.sh
```
Output: `WSL2 Build/x86_64-pc-windows-gnu/release/unreal_devtool.exe`

**Release via CI** — push to `main`, GitHub Actions builds and publishes the `.exe` automatically.

---

## Google Drive upload (rclone)

Uploads use [rclone](https://rclone.org/) — install it once and configure a remote named `gdrive` (or any name you choose):

```powershell
rclone config
```

Follow the prompts: select **Google Drive**, paste your OAuth Client ID and Secret from [Google Cloud Console](https://console.cloud.google.com/), then link your account in the browser tab that opens. Once done, enter a destination in the upload panel:

```
gdrive:/Builds/MyGame
```

rclone must be in your `PATH`. The remote name must match the prefix you used in the destination field.

If an upload fails (expired auth, no permission on the destination, network blocked, etc.), the Status/Output box shows rclone's actual error and a fallback panel offers to open the build folder and Google Drive in your browser for a manual upload, or retry.

---

## Requirements

- Windows 10/11
- [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (pre-installed on Windows 11; free download for Windows 10) — required for Cookie Clicker, 3D Miku, and Sponder Bird embedded panels
- [Ollama](https://ollama.com/) and/or [LM Studio](https://lmstudio.ai/) — optional, only needed for the Dev Assistant chat panel. Neither is bundled; the app just looks for one running locally

---

## Notes

- Config and build names are saved to `%APPDATA%\UnrealDevTool\`
- WebView2 persistent data (Cookie Clicker save, etc.) stored in `%APPDATA%\UnrealDevTool\webview2\`
- Engine detection reads `EngineAssociation` from the `.uproject` file to find the exact matching engine version; a manually-picked engine folder (via Browse…) persists across restarts and always wins over auto-detection until cleared
- Force push to main is intentionally not implemented
- The exe is fully portable — no installer or runtime needed (WebView2 aside)
- The exe bundles rclone.exe (~75 MB), so it's a large download — everything still runs from the single file with no separate assets to manage
- rclone.exe is extracted to `%APPDATA%\UnrealDevtool\` on first use

---

## License

MIT License

Copyright (c) 2026 NickTam

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

---

## Credits

- **Loading animation** — [Ievan Polkka's Hachune Miku Vector (animated)](https://www.deviantart.com/duckne55/art/Ievan-Polkka-s-Hachune-Miku-Vector-animated-451345694) by [duckne55](https://www.deviantart.com/duckne55) on DeviantArt. All rights belong to the original artist.
- **Packaging music** — "Ievan Polkka" from *Hatsune Miku: Project DIVA F Complete Collection*, sourced from [Khinsider](https://downloads.khinsider.com/game-soundtracks/album/hatsune-miku-project-diva-f-complete-collection/2-21.%2520Ievan%2520Polkka.mp3). All rights belong to the original copyright holders.
