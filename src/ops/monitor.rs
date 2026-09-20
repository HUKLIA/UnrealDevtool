//! Live monitoring of an Unreal project: which Unreal processes are running
//! and what they cost, the editor's own log as it is written, and the state of
//! the project folder (sizes, what changed last, crashes).
//!
//! Everything here is passive. It reads the process list, the log file and the
//! folder tree; it never injects into, attaches to, or signals another process.
//! That is deliberate — it is also what keeps a monitor from looking like the
//! things antivirus is trained to flag.
//!
//! The worker only runs while the Monitor sheet is open, and skips the folder
//! scan while a build is running: a packaging run already saturates the disk,
//! and this app has been burned before by adding I/O on top of it.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::ops::run::{level_of, Level};

/// Samples kept for each sparkline: about two minutes at the poll rate.
pub const HISTORY: usize = 90;
/// Log lines retained for display.
const MAX_LOG_LINES: usize = 700;
/// How much of an existing log to read when first attaching. Editor logs grow
/// to tens of megabytes; the last screenful is what matters.
const ATTACH_TAIL_BYTES: u64 = 96 * 1024;
const POLL: Duration = Duration::from_millis(1500);
const HEALTH_EVERY: Duration = Duration::from_secs(12);
/// Ceiling on one folder scan, so a huge project never stalls the worker.
const SCAN_BUDGET: Duration = Duration::from_millis(2500);

/// Processes worth showing, with what they are in plain words.
const WATCHED: &[(&str, &str)] = &[
    ("UnrealEditor.exe",         "Editor"),
    ("UE4Editor.exe",           "Editor"),
    ("UnrealEditor-Cmd.exe",    "Cook / commandlet"),
    ("UE4Editor-Cmd.exe",       "Cook / commandlet"),
    ("ShaderCompileWorker.exe", "Shader compile"),
    ("UnrealBuildTool.exe",     "Build tool"),
    ("AutomationTool.exe",      "Automation (UAT)"),
    ("dotnet.exe",              "dotnet (may be UBT / UAT)"),
    ("cl.exe",                  "C++ compiler"),
    ("link.exe",                "Linker"),
    ("MSBuild.exe",             "MSBuild"),
    ("UnrealPak.exe",           "Pak tool"),
    ("UnrealLightmass.exe",     "Lightmass"),
    ("UnrealTraceServer.exe",   "Trace server"),
    ("ZenServer.exe",           "Zen (shared cache)"),
    ("CrashReportClient.exe",   "Crash reporter"),
];

// ── Data shared with the UI ──────────────────────────────────────────────────

/// All running processes of one image name, added together.
#[derive(Clone, Default, PartialEq)]
pub struct ProcGroup {
    pub image:  String,
    pub role:   &'static str,
    pub count:  usize,
    /// Percent of the whole machine's CPU.
    pub cpu:    f32,
    pub mem_mb: f32,
    /// Age of the longest-running instance.
    pub age_secs: u64,
}

#[derive(Clone)]
pub struct LogEntry {
    pub text:  String,
    pub level: Level,
}

/// One project folder's size.
#[derive(Clone, PartialEq)]
pub struct FolderInfo {
    pub name:    &'static str,
    pub bytes:   u64,
    pub exists:  bool,
    /// The scan hit its time budget, so `bytes` is a lower bound.
    pub partial: bool,
}

/// What the periodic folder scan found.
#[derive(Clone, Default)]
pub struct Health {
    pub folders:      Vec<FolderInfo>,
    /// Most recently modified source-of-truth files: (path relative to the
    /// project, seconds since modified).
    pub recent:       Vec<(String, u64)>,
    pub changed_10m:  usize,
    /// Newest folder under `Saved/Crashes`: (name, seconds ago).
    pub crash:        Option<(String, u64)>,
    pub scan_ms:      u128,
}

#[derive(Default)]
pub struct MonitorData {
    pub procs:     Vec<ProcGroup>,
    /// Combined CPU / memory of every watched process, oldest first.
    pub cpu_hist:  VecDeque<f32>,
    pub mem_hist:  VecDeque<f32>,
    /// The editor's PID and how long it has been up, when it is running.
    pub editor:    Option<(u32, u64)>,

    pub log_path:  Option<PathBuf>,
    pub log:       VecDeque<LogEntry>,
    pub warnings:  u32,
    pub errors:    u32,
    /// Latest work-in-progress counts the editor has logged: shaders still to
    /// compile, and packages still to cook. `None` until one is seen.
    pub shaders_left: Option<u32>,
    pub cook_remaining: Option<u32>,
    log_offset:    u64,
    log_partial:   String,
    log_attached:  bool,

    pub health:    Health,
    pub health_at: Option<Instant>,
    /// Bumped every tick, so the UI can tell fresh data from stale.
    pub ticks:     u64,
}

impl MonitorData {
    fn push_hist(hist: &mut VecDeque<f32>, v: f32) {
        if hist.len() >= HISTORY { hist.pop_front(); }
        hist.push_back(v);
    }
}

// ── Worker ───────────────────────────────────────────────────────────────────

/// Owns the polling thread. Dropping it (or calling [`stop`](Self::stop))
/// ends the thread within a tenth of a second.
pub struct MonitorHandle {
    pub data: Arc<Mutex<MonitorData>>,
    stop:     Arc<AtomicBool>,
    pub project: PathBuf,
}

impl MonitorHandle {
    pub fn start(project: PathBuf, busy: Arc<Mutex<bool>>, ctx: eframe::egui::Context) -> Self {
        let data = Arc::new(Mutex::new(MonitorData::default()));
        let stop = Arc::new(AtomicBool::new(false));
        {
            let (data, stop, project) = (data.clone(), stop.clone(), project.clone());
            std::thread::spawn(move || worker(project, data, stop, busy, ctx));
        }
        Self { data, stop, project }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn worker(
    project: PathBuf,
    data: Arc<Mutex<MonitorData>>,
    stop: Arc<AtomicBool>,
    busy: Arc<Mutex<bool>>,
    ctx: eframe::egui::Context,
) {
    let project_dir = project.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = project.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let log_path = project_dir.join("Saved").join("Logs").join(format!("{stem}.log"));
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;

    // CPU time seen last poll, per PID, to turn totals into a rate.
    let mut last_cpu: HashMap<u32, (u64, Instant)> = HashMap::new();
    let mut last_health: Option<Instant> = None;

    while !stop.load(Ordering::Relaxed) {
        let tick_start = Instant::now();

        let procs = sample_processes(&mut last_cpu, cores);
        let editor = procs.editor;
        let groups = procs.groups;

        {
            let mut d = data.lock().unwrap_or_else(|e| e.into_inner());
            let cpu: f32 = groups.iter().map(|g| g.cpu).sum();
            let mem: f32 = groups.iter().map(|g| g.mem_mb).sum();
            MonitorData::push_hist(&mut d.cpu_hist, cpu);
            MonitorData::push_hist(&mut d.mem_hist, mem);
            d.procs = groups;
            d.editor = editor;
            d.ticks += 1;
            tail_log(&mut d, &log_path);
        }

        let building = *busy.lock().unwrap_or_else(|e| e.into_inner());
        let due = last_health.is_none_or(|t| t.elapsed() >= HEALTH_EVERY);
        if due && !building {
            let health = scan_health(&project_dir);
            let mut d = data.lock().unwrap_or_else(|e| e.into_inner());
            d.health = health;
            d.health_at = Some(Instant::now());
            last_health = Some(Instant::now());
        }

        ctx.request_repaint();

        // Sleep in short slices so closing the sheet stops the worker promptly.
        while tick_start.elapsed() < POLL && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

// ── Processes ────────────────────────────────────────────────────────────────

struct Sampled {
    groups: Vec<ProcGroup>,
    editor: Option<(u32, u64)>,
}

/// Parses `tasklist /FO CSV /NH` into (image, pid, working-set MB).
///
/// Memory is printed with locale-dependent separators ("123,456 K",
/// "123.456 K", "123 456 K"), so only the digits are kept.
pub fn parse_tasklist_csv(text: &str) -> Vec<(String, u32, f32)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let inner = line.strip_prefix('"')?.strip_suffix('"')?;
            let cols: Vec<&str> = inner.split("\",\"").collect();
            if cols.len() < 5 { return None; }
            let pid: u32 = cols[1].trim().parse().ok()?;
            let kb: f32 = cols[4].chars().filter(char::is_ascii_digit).collect::<String>()
                .parse().unwrap_or(0.0);
            Some((cols[0].to_string(), pid, kb / 1024.0))
        })
        .collect()
}

fn role_of(image: &str) -> Option<&'static str> {
    WATCHED.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(image))
        .map(|(_, role)| *role)
}

fn sample_processes(last_cpu: &mut HashMap<u32, (u64, Instant)>, cores: f32) -> Sampled {
    let listing = crate::ops::cmd("tasklist")
        .args(["/fo", "csv", "/nh"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let mut groups: HashMap<String, ProcGroup> = HashMap::new();
    let mut seen: Vec<u32> = Vec::new();
    let mut editor: Option<(u32, u64)> = None;
    let now = Instant::now();

    for (image, pid, mem_mb) in parse_tasklist_csv(&listing) {
        let Some(role) = role_of(&image) else { continue };
        seen.push(pid);

        let times = process_times(pid);
        let cpu_pct = match (times, last_cpu.get(&pid)) {
            (Some(t), Some((prev, at))) => {
                let wall_100ns = now.duration_since(*at).as_nanos() as f64 / 100.0;
                if wall_100ns > 0.0 {
                    (t.cpu_100ns.saturating_sub(*prev) as f64 / wall_100ns / cores as f64 * 100.0) as f32
                } else { 0.0 }
            }
            _ => 0.0,
        };
        if let Some(t) = times {
            last_cpu.insert(pid, (t.cpu_100ns, now));
        }
        let age = times.map(|t| t.age_secs).unwrap_or(0);

        if role == "Editor" && editor.is_none_or(|(_, a)| age > a) {
            editor = Some((pid, age));
        }

        let g = groups.entry(image.to_ascii_lowercase()).or_insert_with(|| ProcGroup {
            image: image.clone(), role, ..Default::default()
        });
        g.count += 1;
        g.cpu += cpu_pct.clamp(0.0, 100.0);
        g.mem_mb += mem_mb;
        g.age_secs = g.age_secs.max(age);
    }
    last_cpu.retain(|pid, _| seen.contains(pid));

    // Editor first, then by CPU.
    let mut list: Vec<ProcGroup> = groups.into_values().collect();
    list.sort_by(|a, b| {
        let rank = |g: &ProcGroup| if g.role == "Editor" { 0 } else { 1 };
        rank(a).cmp(&rank(b))
            .then(b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.image.cmp(&b.image))
    });
    Sampled { groups: list, editor }
}

#[derive(Clone, Copy)]
struct ProcTimes {
    /// Kernel plus user time, in 100 ns units.
    cpu_100ns: u64,
    age_secs:  u64,
}

/// CPU time and age of a process, asked of the OS the way Task Manager does.
/// `None` when the process is gone or protected — the row simply shows 0 %.
fn process_times(pid: u32) -> Option<ProcTimes> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(handle: isize) -> i32;
        fn GetProcessTimes(
            handle: isize, creation: *mut u64, exit: *mut u64, kernel: *mut u64, user: *mut u64,
        ) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    // Seconds between 1601-01-01 (FILETIME epoch) and 1970-01-01.
    const FILETIME_UNIX_DIFF: u64 = 11_644_473_600;

    // SAFETY: plain query calls. The handle is checked before use and closed on
    // every path, and each out-pointer is a live, aligned u64 local.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 { return None; }
        let (mut creation, mut exit, mut kernel, mut user) = (0u64, 0u64, 0u64, 0u64);
        let ok = GetProcessTimes(h, &mut creation, &mut exit, &mut kernel, &mut user);
        CloseHandle(h);
        if ok == 0 { return None; }

        let now_unix = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        let now_ft = (now_unix + FILETIME_UNIX_DIFF) * 10_000_000;
        Some(ProcTimes {
            cpu_100ns: kernel + user,
            age_secs:  now_ft.saturating_sub(creation) / 10_000_000,
        })
    }
}

// ── Editor log ───────────────────────────────────────────────────────────────

/// Shortens Unreal's log prefix: `[2026.09.20-10.11.12:123][456]LogX: text`
/// becomes `10:11:12  LogX: text`. Lines without the prefix pass through.
pub fn tidy_ue_line(line: &str) -> String {
    let line = line.trim_end();
    let Some(rest) = line.strip_prefix('[') else { return line.to_string() };
    let Some((stamp, after)) = rest.split_once(']') else { return line.to_string() };
    // The second bracket is the frame counter; drop it when present.
    let after = match after.strip_prefix('[').and_then(|a| a.split_once(']')) {
        Some((_, tail)) => tail,
        None => after,
    };
    let time = stamp.split_once('-')
        .map(|(_, t)| t.split(':').next().unwrap_or(t).replace('.', ":"));
    match time {
        Some(t) if !after.is_empty() => format!("{t}  {}", after.trim_start()),
        _ => line.to_string(),
    }
}

fn tail_log(d: &mut MonitorData, path: &Path) {
    use std::io::{Read, Seek, SeekFrom};

    if d.log_path.as_deref() != Some(path) {
        d.log_path = Some(path.to_path_buf());
        d.log_attached = false;
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        // No log yet (editor never opened this project): show nothing, and try
        // again next tick.
        d.log_attached = false;
        return;
    };
    let Ok(len) = f.metadata().map(|m| m.len()) else { return };

    // First look at a file, or a fresh editor session replaced it with a
    // shorter one: start from the tail rather than the top.
    let mut skip_partial = false;
    if !d.log_attached || len < d.log_offset {
        d.log.clear();
        d.warnings = 0;
        d.errors = 0;
        d.shaders_left = None;
        d.cook_remaining = None;
        d.log_partial.clear();
        d.log_offset = len.saturating_sub(ATTACH_TAIL_BYTES);
        skip_partial = d.log_offset > 0;
        d.log_attached = true;
    }
    if len == d.log_offset { return; }
    if f.seek(SeekFrom::Start(d.log_offset)).is_err() { return; }
    let mut buf = Vec::new();
    if f.take(1024 * 1024).read_to_end(&mut buf).is_err() { return; }
    d.log_offset += buf.len() as u64;

    let mut acc = std::mem::take(&mut d.log_partial);
    acc.push_str(&String::from_utf8_lossy(&buf));
    let end = acc.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let (complete, rest) = acc.split_at(end);
    d.log_partial = rest.to_string();

    let mut lines = complete.lines();
    if skip_partial { lines.next(); } // began mid-line
    for raw in lines {
        if raw.trim().is_empty() { continue; }
        if let Some(n) = number_before(raw, "shaders left to compile")
            .or_else(|| number_before(raw, "shaders remaining")) {
            d.shaders_left = Some(n);
        }
        if let Some(n) = number_after(raw, "Packages Remain") {
            d.cook_remaining = Some(n);
        }
        let level = level_of(raw);
        match level {
            Level::Warn  => d.warnings += 1,
            Level::Error => d.errors += 1,
            Level::Normal => {}
        }
        if d.log.len() >= MAX_LOG_LINES { d.log.pop_front(); }
        d.log.push_back(LogEntry { text: tidy_ue_line(raw).chars().take(400).collect(), level });
    }
}

/// The integer just before `marker`, e.g. `128` in "128 shaders left to compile".
fn number_before(line: &str, marker: &str) -> Option<u32> {
    let head = line.split(marker).next().filter(|_| line.contains(marker))?;
    head.trim_end().rsplit(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// The integer just after `marker`, e.g. `1730` in "Packages Remain 1730".
fn number_after(line: &str, marker: &str) -> Option<u32> {
    let tail = line.split_once(marker)?.1;
    tail.trim_start().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

// ── Project folder scan ──────────────────────────────────────────────────────

/// Folders whose size is worth knowing, and the ones that hold edits.
const SIZED: [&str; 7] = ["Content", "Source", "Config", "Intermediate", "Saved", "Binaries", "DerivedDataCache"];
const EDITED: [&str; 3] = ["Content", "Source", "Config"];

fn scan_health(project_dir: &Path) -> Health {
    let started = Instant::now();
    let now = SystemTime::now();
    let mut health = Health::default();
    let mut recent: Vec<(SystemTime, PathBuf)> = Vec::new();

    for name in SIZED {
        let dir = project_dir.join(name);
        if !dir.is_dir() {
            health.folders.push(FolderInfo { name, bytes: 0, exists: false, partial: false });
            continue;
        }
        let track = EDITED.contains(&name);
        let (bytes, partial) = walk_size(&dir, started, |path, mtime| {
            if !track { return; }
            if now.duration_since(mtime).is_ok_and(|a| a.as_secs() <= 600) {
                health.changed_10m += 1;
            }
            // Keep a short list of the newest few.
            if recent.len() < 6 || recent.iter().any(|(t, _)| mtime > *t) {
                recent.push((mtime, path.to_path_buf()));
                recent.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
                recent.truncate(6);
            }
        });
        health.folders.push(FolderInfo { name, bytes, exists: true, partial });
    }

    health.recent = recent.into_iter()
        .map(|(t, p)| {
            let rel = p.strip_prefix(project_dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            (rel, now.duration_since(t).map(|d| d.as_secs()).unwrap_or(0))
        })
        .collect();

    // Newest crash folder.
    if let Ok(entries) = std::fs::read_dir(project_dir.join("Saved").join("Crashes")) {
        health.crash = entries.flatten()
            .filter_map(|e| {
                let md = e.metadata().ok()?;
                if !md.is_dir() { return None; }
                Some((md.modified().ok()?, e.file_name().to_string_lossy().to_string()))
            })
            .max_by_key(|(t, _)| *t)
            .map(|(t, n)| (n, now.duration_since(t).map(|d| d.as_secs()).unwrap_or(0)));
    }

    health.scan_ms = started.elapsed().as_millis();
    health
}

/// Total size of a tree, calling `visit` for every file. Stops at the shared
/// time budget and reports that the total is then a lower bound.
fn walk_size(root: &Path, started: Instant, mut visit: impl FnMut(&Path, SystemTime)) -> (u64, bool) {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    let mut n = 0u32;
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            n += 1;
            // Check the clock every so often rather than on every file.
            if n.is_multiple_of(512) && started.elapsed() > SCAN_BUDGET {
                return (total, true);
            }
            let Ok(md) = entry.metadata() else { continue };
            if md.is_dir() {
                stack.push(entry.path());
            } else {
                total += md.len();
                if let Ok(m) = md.modified() {
                    visit(&entry.path(), m);
                }
            }
        }
    }
    (total, false)
}

/// "12 s ago" / "4 min ago" / "3 h ago" / "2 d ago".
pub fn ago(secs: u64) -> String {
    match secs {
        0..=4      => "just now".into(),
        5..=59     => format!("{secs} s ago"),
        60..=3599  => format!("{} min ago", secs / 60),
        3600..=86399 => format!("{} h ago", secs / 3600),
        _          => format!("{} d ago", secs / 86400),
    }
}

/// "3h 12m" / "4m 05s" / "42s".
pub fn uptime(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 { format!("{h}h {m:02}m") } else if m > 0 { format!("{m}m {s:02}s") } else { format!("{s}s") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasklist_csv_is_parsed_whatever_the_locale() {
        let text = "\"UnrealEditor.exe\",\"1234\",\"Console\",\"1\",\"2,345,678 K\"\r\n\
                    \"cl.exe\",\"42\",\"Console\",\"1\",\"12.345 K\"\r\n\
                    garbage line\r\n";
        let rows = parse_tasklist_csv(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "UnrealEditor.exe");
        assert_eq!(rows[0].1, 1234);
        assert!((rows[0].2 - 2290.3).abs() < 0.5, "2,345,678 KB is about 2290 MB");
        assert!((rows[1].2 - 12.05).abs() < 0.1);
    }

    #[test]
    fn only_unreal_processes_are_watched() {
        assert_eq!(role_of("UNREALEDITOR.EXE"), Some("Editor"));
        assert_eq!(role_of("ShaderCompileWorker.exe"), Some("Shader compile"));
        assert_eq!(role_of("chrome.exe"), None);
    }

    #[test]
    fn unreal_log_prefix_is_shortened() {
        assert_eq!(
            tidy_ue_line("[2026.09.20-10.11.12:345][ 67]LogCook: Display: Cooked 12 packages"),
            "10:11:12  LogCook: Display: Cooked 12 packages",
        );
        assert_eq!(tidy_ue_line("LogInit: no prefix here"), "LogInit: no prefix here");
        assert_eq!(tidy_ue_line("[not a timestamp] hello"), "[not a timestamp] hello");
    }

    #[test]
    fn log_tail_starts_at_the_end_of_a_big_file_and_follows_appends() {
        let dir = std::env::temp_dir().join(format!("udt-monitor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("Game.log");

        // A file larger than the attach window: only its tail may be read.
        let mut big = String::new();
        for i in 0..6000 {
            big.push_str(&format!("[2026.09.20-10.00.00:000][{i}]LogTemp: line {i}\n"));
        }
        std::fs::write(&log, &big).unwrap();

        let mut d = MonitorData::default();
        tail_log(&mut d, &log);
        assert!(d.log.len() < 6000, "did not read the whole file");
        assert!(d.log.back().unwrap().text.ends_with("line 5999"));
        assert!(!d.log.front().unwrap().text.is_empty());

        // Appended warning and error lines are counted, a half line is held back.
        let mut more = big.clone();
        more.push_str("[2026.09.20-10.00.01:000][1]LogTemp: Warning: careful\n");
        more.push_str("[2026.09.20-10.00.02:000][2]LogTemp: Error: broken\n");
        more.push_str("[2026.09.20-10.00.03:000][3]LogTemp: partial");
        std::fs::write(&log, &more).unwrap();
        tail_log(&mut d, &log);
        assert_eq!((d.warnings, d.errors), (1, 1));
        assert!(d.log.back().unwrap().text.ends_with("broken"), "unfinished line waits");

        // A new session replaces the file with a shorter one: start over.
        std::fs::write(&log, "[2026.09.20-11.00.00:000][0]LogInit: fresh\n").unwrap();
        tail_log(&mut d, &log);
        assert_eq!(d.log.len(), 1);
        assert_eq!((d.warnings, d.errors), (0, 0));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn folder_scan_sizes_and_finds_recent_edits() {
        let dir = std::env::temp_dir().join(format!("udt-health-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Content/Maps")).unwrap();
        std::fs::create_dir_all(dir.join("Saved/Crashes/UECC-1")).unwrap();
        std::fs::write(dir.join("Content/Maps/Main.umap"), vec![0u8; 5000]).unwrap();

        let h = scan_health(&dir);
        let content = h.folders.iter().find(|f| f.name == "Content").unwrap();
        assert!(content.exists && content.bytes == 5000 && !content.partial);
        assert!(!h.folders.iter().find(|f| f.name == "Binaries").unwrap().exists);
        assert_eq!(h.recent.first().map(|r| r.0.as_str()), Some("Content/Maps/Main.umap"));
        assert_eq!(h.changed_10m, 1);
        assert_eq!(h.crash.as_ref().map(|c| c.0.as_str()), Some("UECC-1"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn time_labels_read_naturally() {
        assert_eq!(ago(2), "just now");
        assert_eq!(ago(90), "1 min ago");
        assert_eq!(ago(7300), "2 h ago");
        assert_eq!(uptime(3900), "1h 05m");
        assert_eq!(uptime(65), "1m 05s");
    }

    #[test]
    fn process_times_read_this_very_process() {
        // Process CPU time advances in ~15 ms ticks; spend a little to be seen.
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(80) { std::hint::black_box(0u64.wrapping_add(1)); }
        let t = process_times(std::process::id()).expect("own process is always readable");
        assert!(t.age_secs < 3600, "a test run is young: {}", t.age_secs);
        assert!(t.cpu_100ns > 0, "the test binary has used some CPU");
        assert!(process_times(u32::MAX - 1).is_none(), "a PID that does not exist is None");
    }

    #[test]
    fn work_in_progress_counts_are_read_from_log_lines() {
        assert_eq!(number_before("LogShaderCompilers: Display: 128 shaders left to compile", "shaders left to compile"), Some(128));
        assert_eq!(number_before("nothing relevant", "shaders left to compile"), None);
        assert_eq!(number_after("LogCook: Cooked packages 4210 Packages Remain 1730 Total 5940", "Packages Remain"), Some(1730));
        assert_eq!(number_after("Packages Remain soon", "Packages Remain"), None);

        let dir = std::env::temp_dir().join(format!("udt-wip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("G.log");
        std::fs::write(&log, "[2026.09.20-10.00.00:000][1]LogShaderCompilers: Display: 40 shaders left to compile
[2026.09.20-10.00.01:000][2]LogCook: Display: Cooked packages 1 Packages Remain 99
").unwrap();
        let mut d = MonitorData::default();
        tail_log(&mut d, &log);
        assert_eq!((d.shaders_left, d.cook_remaining), (Some(40), Some(99)));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
