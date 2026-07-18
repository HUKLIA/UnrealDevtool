# Unreal DevTool

A Windows desktop tool for Unreal Engine 5 developers: packages builds, regenerates Visual Studio project files, manages Git, and includes a few extras — all from one GUI.

> **Study & research project. Not for production.**
> Feel free to use it for testing or as a reference for your own work.

---

## What it does

| | |
|---|---|
| **Rebuild VS Files** | Cleans generated folders, runs `GenerateProjectFiles.bat`, opens the result in Rider or Visual Studio |
| **Package Game** | Runs UAT `BuildCookRun`, renames the output, zips it, and optionally uploads to Google Drive via rclone or copies to a local path. Name the package `TACHYON` (any case) and a one-time trailer video plays in place of the usual GIF for that run |
| **Fast Package** | Same real UAT pipeline as Package Game — progress bar animates at high speed with per-stage sub-bars, GIF plays at 2× speed, and audio plays at 2× speed for the full fast-build feel |
| **Git** | Commit & push, sync with main (fetch → rebase → push, fully automatic), or merge current branch into main |
| **Cookie Clicker** | Embedded Cookie Clicker ([orteil.dashnet.org](https://orteil.dashnet.org/cookieclicker/)) inside the app window with persistent save data across sessions |
| **3D Miku / 2D Miku** | Toggle between the animated 2D Miku GIF and an embedded 3D Unity WebGL viewer with full mouse-look (pointer lock) support |
| **Sponder Bird** | Embedded [Sponder Bird](https://nicktam1.github.io/SponderBirdNew/) game inside the app window |
| **DM on Discord** | Automatically opens Discord on this PC, restores it from minimised/tray, searches for any username via Ctrl+K, and presses Enter to jump straight into the chat. Username is editable in-app |
| **Customize Miku & Sound** | Swap the 2D GIF/image and looping sound for your own files, and pick a custom accent color for the whole app (persisted, with a one-click reset to the default teal) |
| **Quick Links** | One-click buttons to open the Unreal Engine documentation assistant and Claude / ChatGPT / Gemini / Kimi in your default browser |

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

---

## Notes

- Config and build names are saved to `%APPDATA%\UnrealDevTool\`
- WebView2 persistent data (Cookie Clicker save, etc.) stored in `%APPDATA%\UnrealDevTool\webview2\`
- Engine detection reads `EngineAssociation` from the `.uproject` file to find the exact matching engine version
- Force push to main is intentionally not implemented
- The exe is fully portable — no installer or runtime needed (WebView2 aside)
- The exe bundles rclone.exe and the TACHYON trailer video, so it's a large download (~150 MB) — everything still runs from the single file with no separate assets to manage
- rclone.exe and the trailer video are extracted to `%APPDATA%\UnrealDevtool\` on first use

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
