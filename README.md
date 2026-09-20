# Unreal DevTool

**One Windows app that packages your Unreal Engine game and helps with the boring jobs around it.**

Pick your project, press **Start build**, and watch it work. When it finishes you get a zip you can send to a friend, a tester or a store.

![The main screen: choose Windows, Android or Linux, press Start build](docs/images/ready.png)

> **A study and research project.** It works, and it was tested on a real Unreal 5.7 project, but treat it as a helpful tool, not a guaranteed product.

---

## The problem

Making a build of an Unreal game should be easy. In real life it is not:

- **It is slow and silent.** A build takes 10–40 minutes. Unreal's own tools print a wall of text, and you cannot tell if it is stuck or nearly done.
- **It fails with hard-to-read errors.** After 20 minutes you get a cryptic message and no clue what to fix.
- **You have to remember a lot.** Which platform, which settings, which command, which folder, which version number.
- **Small jobs are scattered.** Checking the log, cleaning folders, launching with flags, comparing sizes, switching engines — each one is a different window or a terminal command.

## The goal

Make Unreal packaging **simple and calm** for one person or a small team:

1. **Easy** — one screen, one main button.
2. **Clear** — always show what is happening and what went wrong, in plain words.
3. **Safe** — check things before the build, ask before changing files, keep a backup when it does.
4. **Useful** — put the everyday Unreal jobs in one place.

---

## What you can do

| | |
|---|---|
| **Build your game** | For Windows, Android or Linux, in three ways: full, compile-first, or re-use the last cook (fast). |
| **Watch it live** | See the four stages (Compile, Cook, Stage, Package), a live log and how long each part took. |
| **Understand failures** | When a build fails, the app shows the likely cause and the first error lines. |
| **Check before you build** | Warns about a missing default map, low disk space, a missing Android SDK, and more. Some warnings have a one-click fix. |
| **Monitor your project** | Live view of Unreal processes, the editor log, crashes, and folder sizes. |
| **Run Unreal tools** | Compile all Blueprints, validate data, launch the editor or game with flags, see what makes the package big, switch plugins and engine versions. |
| **Ship it** | Open the folder, upload with rclone, install on an Android phone, copy release notes from your git history. |
| **Do things by typing** | Press `Ctrl+K` to search every action. Press `F1` for a guided tour. |

### A few more screens

| Live build | Finished | Project monitor |
|---|---|---|
| ![Building](docs/images/running.png) | ![Finished](docs/images/done.png) | ![Monitor](docs/images/monitor.png) |

| Unreal tools | Search with Ctrl+K | Works in small windows |
|---|---|---|
| ![Tools](docs/images/tools_commandlets.png) | ![Palette](docs/images/palette.png) | ![Narrow window](docs/images/narrow.png) |

---

## Get started

**You need:** Windows 10 or 11, and Unreal Engine 5 installed. (WebView2 is only needed for the built-in browser and small games; Windows 11 already has it.)

1. Download `unreal_devtool.exe` from the [Releases page](https://github.com/HUKLIA/UnrealDevtool/releases). There is nothing to install — just run it.
2. **Choose your project.** Pick your `.uproject` file, or drag it onto the window. The matching Unreal Engine is found automatically.
3. **Press Start build.** Your zip appears in `build/<version>/` inside your project folder.

That is all. Everything else is optional.

### Useful keys

| Key | What it does |
|---|---|
| `Ctrl+Enter` | Start the build |
| `Ctrl+K` | Search every action, console command or Unreal doc |
| `F1` | Open the guided tour |

### The icons in the top bar

`?` Tour · pulse: Project monitor · wrench: Unreal tools · tick: Project checks · gear: Settings · globe: Browser · bubble: Dev Assistant · dots: Extras

---

## Built with

| Used for | Tool |
|---|---|
| The app itself | **Rust** (edition 2024) |
| The window and buttons | **egui / eframe** |
| The built-in browser | **wry** (Windows WebView2) |
| Sounds | **rodio** |
| Zip files | **zip** (done inside the app, no PowerShell) |
| Updates and downloads | **ureq**, with **sha2** to check the download |
| Reading Unreal's settings | **serde_json**, **winreg** |
| Building Unreal | Unreal's own **RunUAT** and **UnrealBuildTool** |
| Optional upload to Google Drive | **rclone** (you install it yourself) |
| Optional chat helper | **Ollama** or **LM Studio**, running on your own PC |

---

## Known issues and limits

Honest list of what is not perfect:

- **Windows only.** Mac builds cannot be made from Windows (that is an Unreal limit), so that button is greyed out.
- **Android and Linux need their tools installed first** (Android SDK/NDK, or the Linux cross-compile toolchain). The app tells you when they are missing but does not install them.
- **Tested mostly on Windows builds.** The "compile first" and "re-use last cook" methods were run for real on one Unreal 5.7 project. Installing to an Android phone, and the "fix up redirectors" / "resave all packages" tools, have not been tried on real devices or big projects.
- **Windows or your antivirus may warn about it.** The app is not code-signed yet (see below). It is a false alarm, but it can still show a warning.
- **Some tools change your files** (plugins, engine version, default map, `.gitignore`). They always ask first and keep a `.devtool-backup` copy, but commit your work before using them.
- **rclone is not included.** For Google Drive upload you install rclone yourself; the app has a button that opens the download page.

## "My antivirus flags it"

This happens with new, unsigned apps. What has been done to keep it clean: no bundled programs, no hidden PowerShell, no typing into other apps, a proper version and manifest, and every release has a SHA-256 checksum. To check your download:

```powershell
Get-FileHash unreal_devtool.exe -Algorithm SHA256
```

If the result matches the release page, the file is exactly what was built. You can report a false alarm to your antivirus vendor. The full story is in the [technical reference](docs/REFERENCE.md#antivirus-false-positives).

---

## Where it saves things

- Your settings and per-project choices: `%APPDATA%\UnrealDevTool\`
- Builds: `<your project>\build\v0.0.N\` (a zip, the game folder, `BuildLog.txt`, and a small `build-info.txt`)
- Backups of anything the app edits: next to the file, ending in `.devtool-backup`

The app is portable: delete the `.exe` and the settings folder and it is gone.

---

## Build it yourself

You need [Rust](https://rustup.rs/).

```powershell
cargo run              # run a debug build
cargo build --release  # make unreal_devtool.exe in target\release
cargo test             # run the tests
```

**Releases are automatic.** Every push to `main` builds the `.exe` on GitHub Actions and publishes the next version (`v1.0.2`, `v1.0.3`, …) with its checksum. To start a new series, change `version` in `Cargo.toml` (for example to `1.1.0`).

**More detail:** every feature, the design reasons and the file layout are in the [technical reference](docs/REFERENCE.md).

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
- **Fonts** — Space Grotesk and JetBrains Mono, both under the SIL Open Font License (see `Fonts/`).
