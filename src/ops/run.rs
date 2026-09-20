//! Live state of a packaging run: which stage it is in, how long each one
//! took, and the tail of UAT's output.
//!
//! UAT is spawned with its stdout and stderr redirected to a log *file*, not a
//! pipe, so this reads that file as it grows rather than draining a stream.
//! That is deliberate and worth keeping: the full log stays on disk for the
//! diagnostics scanner and for the user to send someone, and a slow reader
//! here can never apply backpressure to the build.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

/// The four phases UAT reports, in the order it runs them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Compile,
    Cook,
    Staging,
    Package,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Compile, Stage::Cook, Stage::Staging, Stage::Package];

    pub fn index(self) -> usize {
        match self {
            Stage::Compile => 0,
            Stage::Cook    => 1,
            Stage::Staging => 2,
            Stage::Package => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Stage::Compile => "Compile",
            Stage::Cook    => "Cook",
            Stage::Staging => "Stage",
            Stage::Package => "Package",
        }
    }
}

/// Severity of a log line, for colouring the output view.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Normal,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct LogLine {
    pub text:  String,
    pub level: Level,
}

/// How many tail lines to keep. The full log is on disk; this is only what the
/// output view shows, and an unbounded buffer on a 30-minute build would grow
/// without limit.
const MAX_LINES: usize = 400;

/// How many whole error lines to remember for the failure summary.
const MAX_ERROR_SAMPLES: usize = 6;

/// Shared between the packaging thread (writer) and the UI (reader).
#[derive(Default)]
pub struct RunProgress {
    pub current:   Option<Stage>,
    /// When each stage began.
    pub started:   [Option<Instant>; 4],
    /// Final duration of each stage that has finished.
    pub elapsed:   [Option<Duration>; 4],
    pub lines:     std::collections::VecDeque<LogLine>,
    /// Counted across the whole run, not just the retained tail.
    pub warnings:  u32,
    pub errors:    u32,
    /// The first few error lines, kept whole. The retained tail rolls over
    /// during a long run, and the line that explains a failure is usually
    /// well before the end.
    pub error_samples: Vec<String>,
    /// Seconds each stage took last time, when known. Replaces the fixed
    /// guesses behind the progress bars with this project's own history.
    typical:       [f32; 4],
    /// Byte offset already consumed from the log file.
    read_offset:   u64,
    /// Partial trailing line carried between reads — a poll can land
    /// mid-line, and splitting on that boundary would corrupt both halves.
    partial:       String,
}

impl RunProgress {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Uses a previous build's stage durations for progress estimates.
    /// Stages that were not reached last time (0) keep the built-in guess.
    pub fn seed_typical(&mut self, secs: [u64; 4]) {
        for (t, s) in self.typical.iter_mut().zip(secs) {
            *t = s as f32;
        }
    }

    /// Duration of a stage: its final time if finished, else live.
    pub fn stage_time(&self, s: Stage) -> Option<Duration> {
        let i = s.index();
        self.elapsed[i].or_else(|| self.started[i].map(|t| t.elapsed()))
    }

    /// Fraction complete for one stage's bar: 1.0 finished, a creeping
    /// estimate while running, 0 before it starts.
    ///
    /// The creep is honest about what is knowable — UAT reports no percentage,
    /// so this is a saturating curve against a typical duration, which moves
    /// continuously and never reaches 1.0 until the stage genuinely ends.
    pub fn stage_fraction(&self, s: Stage) -> f32 {
        let i = s.index();
        if self.elapsed[i].is_some() { return 1.0; }
        let Some(started) = self.started[i] else { return 0.0 };
        let learned = self.typical[i];
        let typical = if learned >= 5.0 { learned } else { match s {
            Stage::Compile => 180.0,
            Stage::Cook    => 420.0,
            Stage::Staging => 90.0,
            Stage::Package => 90.0,
        } };
        let t = started.elapsed().as_secs_f32();
        (1.0 - (-t / typical).exp()) * 0.92
    }

    /// Overall fraction, weighted by how long each stage usually takes.
    pub fn overall(&self) -> f32 {
        const W: [f32; 4] = [0.25, 0.45, 0.15, 0.15];
        Stage::ALL.iter().map(|s| self.stage_fraction(*s) * W[s.index()]).sum()
    }

    fn begin(&mut self, s: Stage) {
        let i = s.index();
        // Close whatever was running: UAT does not interleave phases, so a new
        // one starting is the previous one's end, and this is more reliable
        // than matching its COMPLETED banners (which a failed step may skip).
        if let Some(prev) = self.current
            && prev != s
            && self.elapsed[prev.index()].is_none()
            && let Some(t0) = self.started[prev.index()]
        {
            self.elapsed[prev.index()] = Some(t0.elapsed());
        }
        if self.started[i].is_none() {
            self.started[i] = Some(Instant::now());
        }
        self.current = Some(s);
    }

    /// Marks a stage as begun without waiting for UAT to announce it. Used for
    /// steps that are not UAT at all (UnrealBuildTool run directly), which
    /// print no `COMMAND STARTED` banner.
    pub fn mark_stage(&mut self, s: Stage) {
        self.begin(s);
    }

    /// Marks the run finished, closing the open stage.
    pub fn finish(&mut self) {
        if let Some(cur) = self.current
            && self.elapsed[cur.index()].is_none()
            && let Some(t0) = self.started[cur.index()]
        {
            self.elapsed[cur.index()] = Some(t0.elapsed());
        }
        self.current = None;
    }

    fn push(&mut self, raw: &str) {
        let trimmed = raw.trim_end();
        if trimmed.is_empty() { return; }

        if let Some(s) = stage_of(trimmed) {
            self.begin(s);
        }

        let level = level_of(trimmed);
        match level {
            Level::Warn  => self.warnings += 1,
            Level::Error => {
                self.errors += 1;
                if self.error_samples.len() < MAX_ERROR_SAMPLES {
                    self.error_samples.push(tidy(trimmed));
                }
            }
            Level::Normal => {}
        }

        // UAT prefixes most lines with a timestamp and a source tag that eat
        // most of the width without saying anything. Strip them for display;
        // the file on disk keeps the original.
        let text = tidy(trimmed);
        if text.is_empty() { return; }

        if self.lines.len() >= MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(LogLine { text, level });
    }

    /// Reads whatever has been appended to `path` since the last call.
    ///
    /// Cheap enough for the existing 300ms poll: it seeks to the offset it
    /// stopped at and reads only the new bytes.
    pub fn ingest(&mut self, path: &Path) {
        let Ok(mut f) = std::fs::File::open(path) else { return };
        let Ok(len) = f.metadata().map(|m| m.len()) else { return };

        // The file shrank — a new run reusing the same path. Start over rather
        // than seeking past the end and reading nothing forever.
        if len < self.read_offset {
            self.read_offset = 0;
            self.partial.clear();
        }
        if len == self.read_offset { return; }

        if f.seek(SeekFrom::Start(self.read_offset)).is_err() { return; }
        let mut buf = Vec::new();
        if f.take(512 * 1024).read_to_end(&mut buf).is_err() { return; }
        self.read_offset += buf.len() as u64;

        let chunk = String::from_utf8_lossy(&buf);
        let mut acc = std::mem::take(&mut self.partial);
        acc.push_str(&chunk);

        // Everything up to the last newline is complete; the remainder is
        // carried to the next read.
        let end = acc.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let (complete, rest) = acc.split_at(end);
        self.partial = rest.to_string();

        for line in complete.lines() {
            self.push(line);
        }
    }
}

/// UAT announces each phase with a banner like
/// `********** COOK COMMAND STARTED **********`. Matching those is what makes
/// the stage rail reflect the real build instead of a timer.
fn stage_of(line: &str) -> Option<Stage> {
    if !line.contains("COMMAND STARTED") { return None; }
    let u = line.to_ascii_uppercase();
    if u.contains("BUILD COMMAND")   { return Some(Stage::Compile); }
    if u.contains("COOK COMMAND")    { return Some(Stage::Cook); }
    if u.contains("STAGE COMMAND")   { return Some(Stage::Staging); }
    if u.contains("PACKAGE COMMAND") { return Some(Stage::Package); }
    if u.contains("ARCHIVE COMMAND") { return Some(Stage::Package); }
    None
}

pub fn level_of(line: &str) -> Level {
    let u = line.to_ascii_uppercase();
    if u.contains("ERROR:") || u.contains(": ERROR") || u.contains("FATAL") {
        Level::Error
    } else if u.contains("WARNING:") || u.contains(": WARNING") || u.contains("WARN:") {
        Level::Warn
    } else {
        Level::Normal
    }
}

/// Drops UAT's leading timestamp/tag noise so the visible line starts with
/// what actually happened.
fn tidy(line: &str) -> String {
    let mut s = line.trim();

    // "  2026.09.19-01.20.33: LogCook: ..." — cut at the first ": " that
    // follows a purely timestamp-shaped token.
    if let Some(rest) = s.split_once(further_timestamp_end(s)).map(|(_, r)| r) {
        s = rest.trim_start();
    }
    // "AutomationTool: " / "UAT: " style prefixes add nothing.
    for p in ["AutomationTool: ", "UnrealBuildTool: ", "UAT: ", "LogInit: "] {
        if let Some(rest) = s.strip_prefix(p) {
            s = rest;
        }
    }
    s.chars().take(200).collect()
}

/// Returns the separator to split on, or an empty match when the line has no
/// timestamp prefix. Kept separate so `tidy` stays readable.
fn further_timestamp_end(s: &str) -> &'static str {
    let looks_timestamped = s.len() > 20
        && s.starts_with(|c: char| c.is_ascii_digit())
        && s[..20.min(s.len())].contains('-');
    if looks_timestamped { ": " } else { "\u{0}" }
}
