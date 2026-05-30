# Unreal DevTool

A Windows desktop tool for Unreal Engine 5 developers — packages builds, regenerates Visual Studio project files, and handles Git, all from one GUI.

> **Study & research project. Not for production.**
> Feel free to use it for testing or as a reference for your own work.

---

## What it does

| | |
|---|---|
| **Rebuild VS Files** | Cleans generated folders, runs `GenerateProjectFiles.bat`, opens the result in Rider or Visual Studio |
| **Package Game** | Runs UAT `BuildCookRun`, renames the output, zips it, and optionally uploads to Google Drive or copies to a local path |
| **Git** | Commit & push, sync with main (fetch + rebase), or merge current branch into main |

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

## Notes

- Config and build names are saved to `%APPDATA%\UnrealDevTool\`
- Builds version automatically: `v0.0.1`, `v0.0.2`, …
- Google sign-in happens once; session is cached in `tokencache.json`
- Force push is intentionally not implemented
- The exe is fully portable — no installer or runtime needed
