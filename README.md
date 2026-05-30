# Unreal DevTool

> **Personal study project — not intended for production.**
> Built to practice Rust GUI development with `eframe`/`egui` while solving a real daily-use problem.

A small Windows desktop tool that wraps the three most repetitive Unreal Engine 5 developer tasks:
packaging a game build, regenerating Visual Studio project files, and pushing code to GitHub.
All wrapped in a Hatsune Miku–themed GUI with an animated GIF loading screen.

---

## What it does

| Button | What actually runs |
|---|---|
| **Rebuild Visual Studio Files** | Deletes `Binaries/`, `Intermediate/`, `Saved/`, `.idea/`, `.vs/`, `DerivedDataCache/`, `*.sln`, then runs `GenerateProjectFiles.bat` (or `Build.bat -ProjectFiles` as fallback). Opens the result in Rider or Visual Studio. |
| **Build and Package Game** | Runs `RunUAT.bat BuildCookRun` for Win64, renames the output folder and `.exe`, then zips everything with PowerShell `Compress-Archive`. Auto-increments the build version (`v0.0.1`, `v0.0.2`, …). |
| **Git** | Three-option flow: commit + push to current branch; fetch + rebase on main (sync); or merge current branch into main. Detects conflicts and stops early, asking you to open Fork to resolve. Force push is structurally impossible. |

---

## Things learned while building this

### Rust

- **Module system** — splitting one large `main.rs` into `app.rs`, `ui/`, `ops/`, `types.rs`, etc. and understanding how `pub`, `pub(crate)`, and private visibility work across module boundaries
- **`impl SomeType` across files** — adding methods to a struct from multiple files in different modules works fine as long as it is the same crate
- **`Arc<Mutex<T>>`** — sharing mutable state between the UI thread and background worker threads without `unsafe`
- **Background threads** — `thread::spawn` + closure capture, and why `FnOnce() -> String + Send + 'static` is the right bound
- **`include_bytes!`** — embedding binary assets (the GIF) into the final executable at compile time so no external files are needed
- **Compile-time attributes** — `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` to hide the console only in release builds
- **Static CRT linking** — `.cargo/config.toml` with `target-feature=+crt-static` so the exe runs on machines without the Visual C++ Redistributable
- **Release profile tuning** — `lto = true`, `codegen-units = 1`, `strip = true` in `Cargo.toml`

### egui / eframe

- **Retained-mode vs immediate-mode** — egui redraws the entire frame every tick; state lives in the app struct, not in widgets
- **`Arc<Mutex<String>>` as a live status channel** — the background thread writes to it, the UI thread reads it every frame with `.clone()`; `ctx.request_repaint()` keeps the UI painting while a task is running
- **Splitting UI into methods** — `impl DevToolApp` blocks can be spread across multiple files; each `ui/` file adds its own panel methods
- **State machine UI** — `GitState` enum drives which panel is visible; transitions happen inside `Frame::show()` closures by mutating `self.git_state` directly
- **Frame builders as helpers** — `fn git_frame() -> egui::Frame` returns a pre-styled frame so every git panel shares the same border/fill without repeating the styling code
- **`add_enabled_ui`** — graying out a whole group of widgets with one call
- **`egui::TextureHandle`** — uploading a new texture each GIF frame and letting the old handle drop automatically
- **`ctx.input(|i| i.key_pressed(...))`** — reading keyboard state inside the closure egui provides for input queries

### GIF playback in egui

- Decoded all frames from `include_bytes!` at startup using `image::codecs::gif::GifDecoder` + `AnimationDecoder::into_frames()`
- Each frame stored as `egui::ColorImage` (a `Vec<Color32>`)
- Each tick: accumulate `ctx.input(|i| i.stable_dt)`, advance frame index when elapsed ≥ delay, upload new `TextureHandle` only when the frame changes
- `ctx.request_repaint()` called every tick so egui keeps painting while the GIF plays

### Windows-specific

- **Registry reads with `winreg`** — scanning `HKLM\SOFTWARE\EpicGames\Unreal Engine\5.x` to find the engine path without hardcoding it
- **`rfd::FileDialog`** — native Windows file picker dialog, called synchronously from the UI thread (it blocks briefly while the dialog is open, which is expected behavior)
- **Spawning batch files** — `std::process::Command::new("cmd").args(["/c", "RunUAT.bat", ...])` is the correct way to run `.bat` files from Rust on Windows
- **`$ErrorActionPreference='Stop'` in PowerShell** — makes `Compress-Archive` propagate errors as a non-zero exit code instead of swallowing them silently

---

## Build

```bash
# Debug (console window visible, good for development)
cargo run

# Release (no console, static CRT, stripped symbols — single portable .exe)
cargo build --release
# output: target\release\unreal_devtool.exe
```

The release binary needs no installer and no Visual C++ Redistributable.
It only requires `opengl32.dll` and `gdi32.dll`, which ship with every version of Windows.

---

## Notes

- The `.uproject` path and per-project build names (package folder name, exe name) are
  saved to `%APPDATA%\UnrealDevTool\` so they persist across restarts.
- Build outputs go to `{project_dir}\build\v0.0.{N}\` where N auto-increments.
- Git force push is not implemented anywhere in the codebase — intentionally.
- On git conflict, the tool stops and tells you to open Fork. It never runs
  `git rebase --abort` or `git merge --abort` automatically.
- The Hatsune Miku GIF (`miku-hatsune.gif`, 200×168 px, ~40 KB) is embedded in the
  binary and displayed at 1.5× (300×252 px) while any background task is running.
