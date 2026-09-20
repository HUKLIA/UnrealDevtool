//! Embeds Windows resources into the executable: a version-info block, an
//! application manifest and an icon.
//!
//! This is antivirus-facing work, not cosmetics. A Rust GUI binary with no
//! version resource has no CompanyName, no ProductName, no FileDescription
//! and no icon — Explorer shows it as an anonymous generic executable, and
//! reputation-based engines (Defender's SmartScreen and ML models especially)
//! weight exactly those fields when scoring an unsigned binary they have not
//! seen before. Populating them does not make the app *trusted*, but it moves
//! it out of the "anonymous blob" bucket that gets flagged on sight.
//!
//! The real fix for a false positive is an Authenticode signature from a code
//! signing certificate; see README for how to wire one into the release
//! workflow. Everything here is what can be done without one.

fn main() {
    // The resource compiler is Windows-only. The WSL2 cross-build
    // (`build.sh`, x86_64-pc-windows-gnu) runs this on Linux, where
    // winresource cannot invoke a resource compiler — skip rather than fail
    // the build.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if !cfg!(target_os = "windows") {
        return;
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Image/app-icon-head.ico");
    println!("cargo:rerun-if-changed=app.manifest");

    let mut res = winresource::WindowsResource::new();
    res.set_icon("Image/app-icon-head.ico");
    res.set_manifest_file("app.manifest");

    // Fields Explorer, SmartScreen and most scanners read off the binary.
    res.set("ProductName",      "Unreal DevTool");
    res.set("FileDescription",  "Unreal Engine project packaging and git helper");
    res.set("CompanyName",      "Unreal DevTool");
    res.set("LegalCopyright",   "MIT licensed. See the project repository.");
    res.set("OriginalFilename", "unreal_devtool.exe");
    res.set("InternalName",     "unreal_devtool");

    if let Err(e) = res.compile() {
        // A missing resource compiler should not break a developer's build —
        // the resulting exe is just missing metadata, which only matters for
        // the binaries actually shipped from CI.
        println!("cargo:warning=could not embed Windows resources: {e}");
    }
}
