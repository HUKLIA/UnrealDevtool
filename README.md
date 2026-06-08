# Unreal DevTool

A Windows desktop tool for Unreal Engine 5 developers: Packages builds, regenerates Visual Studio project files, and handles Git, all from one GUI.

> **Study & research project. Not for production.**
> Feel free to use it for testing or as a reference for your own work.

---

## What it does

| | |
|---|---|
| **Rebuild VS Files** | Cleans generated folders, runs `GenerateProjectFiles.bat`, opens the result in Rider or Visual Studio |
| **Package Game** | Runs UAT `BuildCookRun`, renames the output, zips it, and optionally uploads to Google Drive via rclone or copies to a local path |
| **Git** | Commit & push, sync with main (fetch + rebase), or merge current branch into main |
| **Cookie Clicker** | Opens [Cookie Clicker](https://orteil.dashnet.org/cookieclicker/) in the default browser |
| **DM Spencer** | Step-by-step guide for DMing Spencer (`gonkindroid`) on Discord, with a shortcut to [Sponder Bird](https://nicktam1.github.io/SponderBirdNew/) |

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

---

## Notes

- Config and build names are saved to `%APPDATA%\UnrealDevTool\`
- Builds version automatically: `v0.0.1`, `v0.0.2`, …
- Google Drive uploads use rclone — no OAuth tokens stored by the app
- Force push is intentionally not implemented
- The exe is fully portable — no installer or runtime needed

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
