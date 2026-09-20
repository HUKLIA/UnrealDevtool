//! Where the bytes are: a size breakdown of a project's Content folder or of a
//! packaged build.
//!
//! Package size is the number people ask about last and regret first. This
//! answers "what is big?" — by kind of file, by top-level folder, and the
//! largest individual files — so the next step is obvious (a 900 MB uncompressed
//! texture, a movie nobody uses, a second copy of a pak).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Files reported individually.
const TOP_FILES: usize = 25;
/// Top-level folders reported.
const TOP_DIRS: usize = 14;
/// Longest a scan may run before it reports what it has.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(20);

#[derive(Clone, Default)]
pub struct Insights {
    pub root:      PathBuf,
    pub total:     u64,
    pub files:     usize,
    /// (kind, bytes, file count), largest first.
    pub by_kind:   Vec<(String, u64, usize)>,
    /// (path relative to the root, bytes), largest first.
    pub top_files: Vec<(String, u64)>,
    /// (top-level folder, bytes), largest first.
    pub top_dirs:  Vec<(String, u64)>,
    /// The scan hit its time limit, so every figure is a lower bound.
    pub partial:   bool,
    pub secs:      f32,
}

/// What kind of file this is, in the terms an Unreal developer thinks in.
pub fn kind_of(path: &Path) -> &'static str {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "uasset" | "umap"                       => "Assets (.uasset / .umap)",
        "uexp"                                  => "Asset data (.uexp)",
        "ubulk" | "uptnl"                       => "Bulk data (.ubulk)",
        "pak" | "utoc" | "ucas"                 => "Packed content (.pak / .ucas)",
        "exe" | "dll" | "so" | "apk" | "aab"    => "Binaries",
        "png" | "jpg" | "jpeg" | "tga" | "psd" | "exr" | "hdr" | "bmp" => "Source images",
        "wav" | "mp3" | "ogg" | "flac" | "bnk"  => "Audio",
        "mp4" | "mov" | "avi" | "webm" | "bk2"  => "Video",
        "fbx" | "obj" | "abc" | "usd" | "usda" | "usdc" | "gltf" | "glb" => "Source meshes",
        ""                                      => "No extension",
        _                                       => "Other",
    }
}

/// Measures `root`. Blocking — call it from a background thread.
pub fn scan(root: &Path, budget: Duration) -> Insights {
    let started = Instant::now();
    let mut out = Insights { root: root.to_path_buf(), ..Default::default() };
    let mut kinds: HashMap<&'static str, (u64, usize)> = HashMap::new();
    let mut dirs: HashMap<String, u64> = HashMap::new();
    let mut top: Vec<(String, u64)> = Vec::new();

    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0u32;
    'walk: while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            seen += 1;
            if seen.is_multiple_of(256) && started.elapsed() > budget {
                out.partial = true;
                break 'walk;
            }
            let Ok(md) = entry.metadata() else { continue };
            let path = entry.path();
            if md.is_dir() {
                stack.push(path);
                continue;
            }
            let size = md.len();
            out.total += size;
            out.files += 1;
            let k = kinds.entry(kind_of(&path)).or_insert((0, 0));
            k.0 += size;
            k.1 += 1;

            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            let first = rel_s.split('/').next().unwrap_or("").to_string();
            // Files directly in the root are grouped so they still add up.
            let key = if rel_s.contains('/') { first } else { "(root files)".to_string() };
            *dirs.entry(key).or_insert(0) += size;

            // Keep only the biggest few, without sorting every file.
            if top.len() < TOP_FILES || top.last().is_some_and(|(_, s)| size > *s) {
                top.push((rel_s, size));
                top.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
                top.truncate(TOP_FILES);
            }
        }
    }

    out.by_kind = kinds.into_iter().map(|(k, (b, n))| (k.to_string(), b, n)).collect();
    out.by_kind.sort_by_key(|(_, b, _)| std::cmp::Reverse(*b));
    let mut d: Vec<(String, u64)> = dirs.into_iter().collect();
    d.sort_by_key(|(_, b)| std::cmp::Reverse(*b));
    d.truncate(TOP_DIRS);
    out.top_dirs = d;
    out.top_files = top;
    out.secs = started.elapsed().as_secs_f32();
    out
}

/// What changed between two builds (or two scans of the same folder).
#[derive(Clone, Default)]
pub struct Comparison {
    pub older_total: u64,
    pub newer_total: u64,
    /// (kind, change in bytes), biggest change first.
    pub kinds: Vec<(String, i64)>,
    /// (folder two levels deep, change in bytes), biggest change first.
    pub dirs:  Vec<(String, i64)>,
    /// Files that grew or appeared: (path, change, is new).
    pub grown: Vec<(String, i64, bool)>,
    /// Files that shrank or vanished: (path, change, was removed).
    pub shrunk: Vec<(String, i64, bool)>,
    pub partial: bool,
}

/// Every file under `root` as (relative path, size).
fn inventory(root: &Path, budget: Duration) -> (HashMap<String, u64>, bool) {
    let started = Instant::now();
    let mut files = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0u32;
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen.is_multiple_of(256) && started.elapsed() > budget { return (files, true); }
            let Ok(md) = e.metadata() else { continue };
            let p = e.path();
            if md.is_dir() { stack.push(p); continue; }
            let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            files.insert(rel, md.len());
        }
    }
    (files, false)
}

/// Compares two folders: what grew, what shrank, and where. Package size creeps
/// up a little every build; this says which build made it jump and why.
pub fn compare(older: &Path, newer: &Path, budget: Duration) -> Comparison {
    let (a, pa) = inventory(older, budget);
    let (b, pb) = inventory(newer, budget);
    let mut out = Comparison {
        older_total: a.values().sum(),
        newer_total: b.values().sum(),
        partial: pa || pb,
        ..Default::default()
    };

    let mut kinds: HashMap<&'static str, i64> = HashMap::new();
    let mut dirs: HashMap<String, i64> = HashMap::new();
    let mut changes: Vec<(String, i64, bool)> = Vec::new(); // (path, delta, appeared-or-vanished)

    let mut paths: Vec<&String> = a.keys().chain(b.keys()).collect();
    paths.sort();
    paths.dedup();
    for p in paths {
        let (old, new) = (a.get(p).copied(), b.get(p).copied());
        let delta = new.unwrap_or(0) as i64 - old.unwrap_or(0) as i64;
        if delta == 0 { continue; }
        *kinds.entry(kind_of(Path::new(p))).or_insert(0) += delta;
        let key: String = p.split('/').take(2).collect::<Vec<_>>().join("/");
        *dirs.entry(key).or_insert(0) += delta;
        changes.push((p.clone(), delta, old.is_none() || new.is_none()));
    }

    let by_size = |mut v: Vec<(String, i64)>| { v.sort_by_key(|(_, d)| std::cmp::Reverse(d.abs())); v.truncate(TOP_DIRS); v };
    out.kinds = by_size(kinds.into_iter().map(|(k, d)| (k.to_string(), d)).collect());
    out.dirs = by_size(dirs.into_iter().collect());

    changes.sort_by_key(|(_, d, _)| std::cmp::Reverse(d.abs()));
    out.grown = changes.iter().filter(|(_, d, _)| *d > 0).take(12).cloned().collect();
    out.shrunk = changes.iter().filter(|(_, d, _)| *d < 0).take(8).cloned().collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_is_broken_down_by_kind_folder_and_size() {
        let root = std::env::temp_dir().join(format!("udt-insights-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Maps")).unwrap();
        std::fs::create_dir_all(root.join("Audio/Music")).unwrap();
        std::fs::write(root.join("Maps/Main.umap"), vec![0u8; 1000]).unwrap();
        std::fs::write(root.join("Maps/Main.uexp"), vec![0u8; 4000]).unwrap();
        std::fs::write(root.join("Audio/Music/Theme.wav"), vec![0u8; 9000]).unwrap();
        std::fs::write(root.join("readme"), vec![0u8; 10]).unwrap();

        let i = scan(&root, DEFAULT_BUDGET);
        assert_eq!((i.total, i.files, i.partial), (14_010, 4, false));
        assert_eq!(i.top_files[0], ("Audio/Music/Theme.wav".to_string(), 9000));
        assert_eq!(i.top_dirs[0], ("Audio".to_string(), 9000));
        assert!(i.top_dirs.iter().any(|(d, b)| d == "(root files)" && *b == 10));
        assert_eq!(i.by_kind[0].0, "Audio");
        assert!(i.by_kind.iter().any(|(k, b, n)| k.starts_with("Asset data") && *b == 4000 && *n == 1));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_the_largest_files_are_kept() {
        let root = std::env::temp_dir().join(format!("udt-insights-top-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for n in 0..40u64 {
            std::fs::write(root.join(format!("f{n}.bin")), vec![0u8; (n as usize + 1) * 10]).unwrap();
        }
        let i = scan(&root, DEFAULT_BUDGET);
        assert_eq!(i.top_files.len(), TOP_FILES);
        assert_eq!(i.top_files[0].1, 400, "largest first");
        assert!(i.top_files.iter().all(|(_, s)| *s >= 160), "the small ones were dropped");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn two_builds_are_compared_by_kind_folder_and_file() {
        let base = std::env::temp_dir().join(format!("udt-compare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (old, new) = (base.join("old"), base.join("new"));
        for d in [&old, &new] { std::fs::create_dir_all(d.join("Game/Content/Paks")).unwrap(); }
        std::fs::write(old.join("Game/Content/Paks/main.pak"), vec![0u8; 1000]).unwrap();
        std::fs::write(new.join("Game/Content/Paks/main.pak"), vec![0u8; 1800]).unwrap();
        std::fs::write(old.join("Game/removed.dll"), vec![0u8; 300]).unwrap();
        std::fs::write(new.join("Game/added.wav"), vec![0u8; 500]).unwrap();
        std::fs::write(old.join("Game/same.bin"), vec![0u8; 50]).unwrap();
        std::fs::write(new.join("Game/same.bin"), vec![0u8; 50]).unwrap();

        let c = compare(&old, &new, DEFAULT_BUDGET);
        assert_eq!((c.older_total, c.newer_total), (1350, 2350));
        assert_eq!(c.grown[0], ("Game/Content/Paks/main.pak".to_string(), 800, false));
        assert!(c.grown.iter().any(|(p, d, new)| p == "Game/added.wav" && *d == 500 && *new));
        assert_eq!(c.shrunk, vec![("Game/removed.dll".to_string(), -300, true)]);
        assert_eq!(c.dirs[0], ("Game/Content".to_string(), 800));
        assert!(c.kinds.iter().any(|(k, d)| k == "Audio" && *d == 500));
        assert!(!c.grown.iter().any(|(p, _, _)| p == "Game/same.bin"), "unchanged files are left out");
        std::fs::remove_dir_all(base).unwrap();
    }
}
