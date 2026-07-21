use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::types::GitTaskStatus;

// ── Low-level git helpers ─────────────────────────────────────────────────────

pub fn run_git(dir: &Path, args: &[&str]) -> (bool, String) {
    match crate::ops::cmd("git").args(args).current_dir(dir).output() {
        Err(e) => (false, format!("git not found: {}", e)),
        Ok(o)  => {
            let out = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr),
            );
            (o.status.success(), out)
        }
    }
}

pub fn is_conflict(output: &str) -> bool {
    output.contains("CONFLICT")
        || output.contains("Automatic merge failed")
        || output.contains("cannot rebase: You have unstaged changes")
        || (output.to_lowercase().contains("conflict") && output.contains("Merge"))
}

pub fn git_current_branch(dir: &Path) -> String {
    let (ok, out) = run_git(dir, &["branch", "--show-current"]);
    if ok { out.trim().to_string() } else { "unknown".to_string() }
}

/// Real, locally-derivable repo status for the Git tab's companion panel —
/// no field here is fabricated: uncommitted count and last commit come
/// straight from `git status`/`git log`, and ahead/behind comes from the
/// *local* tracking ref, so it's only as fresh as the last fetch (labeled
/// as such wherever it's displayed, not presented as live remote state).
///
/// `activity`/`insertions`/`deletions`/`changed_files` back the Git tab's
/// bar-chart panel (`ui::bar_chart::show_bar_chart`) — same rule applies:
/// every number is derived from an actual git command, and a failed command
/// (no repo, no commits, git missing) leaves the field at its empty/zero
/// default rather than inventing a placeholder value. The render side is
/// responsible for telling "genuinely zero" apart from "couldn't be
/// measured" (e.g. `activity` empty means "no data", not "zero commits").
#[derive(Clone, Default)]
pub struct GitStatusSummary {
    pub uncommitted:    usize,
    pub last_commit:    Option<String>,
    pub ahead_behind:   Option<(usize, usize)>,
    pub activity:       Vec<(String, usize)>,
    pub insertions:     usize,
    pub deletions:      usize,
    pub changed_files:  usize,
}

pub fn git_status_summary(dir: &Path) -> GitStatusSummary {
    let (ok, out) = run_git(dir, &["status", "--porcelain"]);
    let uncommitted = if ok { out.lines().filter(|l| !l.trim().is_empty()).count() } else { 0 };

    let (ok, out) = run_git(dir, &["log", "-1", "--format=%h  %s  (%cr)"]);
    let last_commit = if ok && !out.trim().is_empty() { Some(out.trim().to_string()) } else { None };

    let (ok, out) = run_git(dir, &["rev-list", "--left-right", "--count", "@{u}...HEAD"]);
    let ahead_behind = if ok {
        let parts: Vec<&str> = out.trim().split_whitespace().collect();
        match (parts.first().and_then(|s| s.parse().ok()), parts.get(1).and_then(|s| s.parse().ok())) {
            (Some(behind), Some(ahead)) => Some((ahead, behind)),
            _ => None,
        }
    } else {
        None
    };

    let activity = git_activity_last_14_days(dir);
    let (insertions, deletions, changed_files) = git_working_tree_diffstat(dir);

    GitStatusSummary { uncommitted, last_commit, ahead_behind, activity, insertions, deletions, changed_files }
}

/// Commit counts per day for the last 14 days, oldest first, one entry per
/// day even when that day had zero commits — so the bar chart always gets
/// 14 evenly spaced bars instead of a sparse, unevenly-gapped set. Returns
/// an empty `Vec` (never a fabricated all-zero row) if `git log` itself
/// fails — no repo, no commits at all, or git not installed — so the
/// caller can render "no data" instead of a misleading flat chart.
fn git_activity_last_14_days(dir: &Path) -> Vec<(String, usize)> {
    // `--date=short-local`, not plain `--date=short`: the unsuffixed form
    // renders each commit in *the committer's own recorded timezone*, so a
    // teammate's commit lands in a bucket keyed to their calendar day, not
    // the one this user is looking at. `-local` normalizes every commit to
    // the viewer's timezone, which is also the timezone `today_days` below
    // is resolved in — both sides of the lookup have to agree or the counts
    // silently land in the wrong bar.
    let (ok, out) = run_git(dir, &["log", "--since=14 days ago", "--date=short-local", "--pretty=format:%cd"]);
    if !ok {
        return Vec::new();
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for line in out.lines() {
        let d = line.trim();
        if !d.is_empty() {
            *counts.entry(d.to_string()).or_insert(0) += 1;
        }
    }

    // "Today" as days-since-epoch, computed without pulling in a date/time
    // crate just for this one chart — `civil_from_days` below turns it (and
    // each of the 13 preceding days) back into a calendar date to build the
    // key `git --date=short-local` uses ("YYYY-MM-DD") and the short axis
    // label ("MM-DD").
    //
    // This has to be the *local* day, not the UTC one. Deriving it from
    // `SystemTime::now()` alone gives UTC, and for anyone not on UTC that
    // disagrees with `-local` for part of every day — at UTC+8, between
    // local midnight and 08:00 the UTC date is still yesterday, so the
    // whole 14-day window shifted back one bar and the commits made so far
    // today fell outside it entirely and vanished from the chart.
    let today_days = match local_today_days(dir) {
        Some(d) => d,
        None    => return Vec::new(),
    };

    (0..14i64)
        .rev()
        .map(|offset| {
            let (y, m, d) = civil_from_days(today_days - offset);
            let key   = format!("{:04}-{:02}-{:02}", y, m, d);
            let label = format!("{:02}-{:02}", m, d);
            let count = counts.get(&key).copied().unwrap_or(0);
            (label, count)
        })
        .collect()
}

/// Days-since-Unix-epoch → Gregorian calendar (year, month, day). Howard
/// Hinnant's well-known constant-time civil-calendar algorithm (public
/// domain) — used instead of adding a date/time crate dependency just to
/// answer "what calendar date is N days before today" for the activity
/// chart's axis.
/// Today as days-since-epoch *in the user's local timezone*, to match the
/// dates `git log --date=short-local` emits.
///
/// Rust's std has no timezone API, and this app has no date crate — but git
/// itself already knows the local offset, and `git var GIT_AUTHOR_IDENT`
/// reports it alongside the current time as the trailing two fields of an
/// ident line:
///
///     Some Name <a@b.c> 1784644585 -0700
///
/// Falls back to the UTC day if that output can't be parsed, which is only
/// ever off by one near midnight — strictly better than dropping the whole
/// chart.
fn local_today_days(dir: &Path) -> Option<i64> {
    let utc_days = || {
        SystemTime::now().duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| (d.as_secs() / 86_400) as i64)
    };

    let (ok, out) = run_git(dir, &["var", "GIT_AUTHOR_IDENT"]);
    if !ok {
        return utc_days();
    }

    // Parse from the right: the name/email prefix is free-form and may
    // itself contain spaces, but the timestamp and offset are always the
    // final two fields.
    let fields: Vec<&str> = out.trim().split_whitespace().collect();
    let parsed = match (fields.len() >= 2, fields.last(), fields.get(fields.len().wrapping_sub(2))) {
        (true, Some(offset), Some(ts)) => {
            let secs: i64 = match ts.parse() {
                Ok(s)  => s,
                Err(_) => return utc_days(),
            };
            let sign = match offset.as_bytes().first() {
                Some(b'-') => -1,
                Some(b'+') =>  1,
                _          => return utc_days(),
            };
            // ±HHMM
            let digits = &offset[1..];
            if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return utc_days();
            }
            let hh: i64 = digits[..2].parse().ok()?;
            let mm: i64 = digits[2..].parse().ok()?;
            Some(secs + sign * (hh * 3600 + mm * 60))
        }
        _ => None,
    };

    match parsed {
        // Floor-divide: `/` truncates toward zero, which would land on the
        // wrong day for pre-1970 local timestamps.
        Some(local_secs) => Some(local_secs.div_euclid(86_400)),
        None             => utc_days(),
    }
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y   = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m   = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y   = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Working-tree changes vs. `HEAD` (`git diff --numstat HEAD`): total lines
/// added/removed and how many files changed. `changed_files` counts every
/// numstat row (including binary files, which numstat reports as `-  -
/// path`); `insertions`/`deletions` only sum the rows that parse as real
/// numbers, silently skipping binary rows' `-` columns rather than treating
/// them as zero. Fails closed to `(0, 0, 0)` — no repo, no `HEAD` yet (a
/// brand new repo with no commits), or git missing all land here, and the
/// caller shows "no data" rather than a real-looking zero.
fn git_working_tree_diffstat(dir: &Path) -> (usize, usize, usize) {
    let (ok, out) = run_git(dir, &["diff", "--numstat", "HEAD"]);
    if !ok {
        return (0, 0, 0);
    }

    let mut insertions    = 0usize;
    let mut deletions     = 0usize;
    let mut changed_files = 0usize;
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        changed_files += 1;
        let mut cols = line.splitn(3, '\t');
        let ins = cols.next().unwrap_or("-");
        let del = cols.next().unwrap_or("-");
        if let Ok(i) = ins.parse::<usize>() { insertions += i; }
        if let Ok(d) = del.parse::<usize>() { deletions  += d; }
    }
    (insertions, deletions, changed_files)
}

// ── Background task functions (run on worker thread) ─────────────────────────

pub fn task_git_commit_push(
    dir:      PathBuf,
    msg:      String,
    branch:   String,
    status:   Arc<Mutex<String>>,
    result:   Arc<Mutex<Option<GitTaskStatus>>>,
    cancel:   Arc<AtomicBool>,
    progress: Arc<Mutex<f32>>,
) -> String {
    macro_rules! upd   { ($s:expr) => { *status.lock().unwrap() = $s.to_string(); }; }
    macro_rules! prog  { ($v:expr) => { *progress.lock().unwrap() = $v; }; }
    macro_rules! check { () => { if cancel.load(Ordering::Relaxed) {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return "[CANCELLED] Git operation was cancelled.".to_string();
    }}; }

    check!(); prog!(0.05);
    upd!("[1/4] Staging all changes…");
    let (ok, out) = run_git(&dir, &["add", "."]);
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] git add: {}", out);
    }

    let (_, porcelain) = run_git(&dir, &["status", "--porcelain"]);
    let has_changes = !porcelain.trim().is_empty();

    check!(); prog!(0.20);
    if has_changes {
        upd!(format!("[2/4] Committing: \"{}\"…", msg));
        let (ok, out) = run_git(&dir, &["commit", "-m", &msg]);
        if !ok {
            *result.lock().unwrap() = Some(GitTaskStatus::Error);
            return format!("[ERROR] git commit: {}", out);
        }
    } else {
        upd!("[2/4] Nothing to commit — skipping commit step.");
    }

    check!(); prog!(0.50);
    upd!(format!("[3/4] Pushing to origin/{}…", branch));
    let (ok, out) = run_git(&dir, &["push", "-u", "origin", &branch]);
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] git push failed:\n{}\n⚠ Force push is never allowed.", out);
    }

    prog!(0.80);
    upd!("[4/4] Fetching to update remote tracking…");
    let _ = run_git(&dir, &["fetch", "origin"]);

    prog!(1.0);
    *result.lock().unwrap() = Some(GitTaskStatus::Ok);
    if has_changes {
        format!("[DONE] Committed & pushed to  {}", branch)
    } else {
        format!("[DONE] Pushed (no new commit) to  {}", branch)
    }
}

pub fn task_git_sync(
    dir:      PathBuf,
    status:   Arc<Mutex<String>>,
    result:   Arc<Mutex<Option<GitTaskStatus>>>,
    cancel:   Arc<AtomicBool>,
    progress: Arc<Mutex<f32>>,
) -> String {
    macro_rules! upd   { ($s:expr) => { *status.lock().unwrap() = $s.to_string(); }; }
    macro_rules! prog  { ($v:expr) => { *progress.lock().unwrap() = $v; }; }
    macro_rules! check { () => { if cancel.load(Ordering::Relaxed) {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return "[CANCELLED] Git operation was cancelled.".to_string();
    }}; }

    let branch = git_current_branch(&dir);

    check!(); prog!(0.05);
    upd!("[1/3] Fetching origin/main…");
    let (ok, out) = run_git(&dir, &["fetch", "origin", "main"]);
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] fetch: {}", out);
    }

    check!(); prog!(0.40);
    upd!("[2/3] Rebasing onto origin/main…");
    let (ok, out) = run_git(&dir, &["rebase", "origin/main"]);
    if !ok {
        if is_conflict(&out) {
            *result.lock().unwrap() = Some(GitTaskStatus::Conflict);
            return format!(
                "⚠ Rebase conflict!\n{}\n\nOpen Fork to resolve, then run:\n  git rebase --continue",
                out.trim()
            );
        }
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] rebase: {}", out);
    }

    check!(); prog!(0.75);
    upd!(format!("[3/3] Pushing {} to origin…", branch));
    let branch_str = branch.as_str();
    let is_main_branch = branch_str == "main" || branch_str == "master";
    let (ok, out) = if is_main_branch {
        run_git(&dir, &["push", "origin", branch_str])
    } else {
        run_git(&dir, &["push", "--force-with-lease", "origin", branch_str])
    };
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] push: {}", out);
    }

    prog!(1.0);
    *result.lock().unwrap() = Some(GitTaskStatus::Ok);
    format!("[DONE] Synced with main and pushed {} to origin.", branch)
}

pub fn task_git_merge_to_main(
    dir:         PathBuf,
    from_branch: String,
    status:      Arc<Mutex<String>>,
    result:      Arc<Mutex<Option<GitTaskStatus>>>,
    cancel:      Arc<AtomicBool>,
    progress:    Arc<Mutex<f32>>,
) -> String {
    macro_rules! upd   { ($s:expr) => { *status.lock().unwrap() = $s.to_string(); }; }
    macro_rules! prog  { ($v:expr) => { *progress.lock().unwrap() = $v; }; }
    macro_rules! check { () => { if cancel.load(Ordering::Relaxed) {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return "[CANCELLED] Git operation was cancelled.".to_string();
    }}; }

    check!(); prog!(0.05);
    upd!("[1/4] Switching to main…");
    let (ok, out) = run_git(&dir, &["checkout", "main"]);
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] checkout main: {}", out);
    }

    check!(); prog!(0.25);
    upd!("[2/4] Pulling latest main…");
    let (ok, out) = run_git(&dir, &["pull", "origin", "main"]);
    if !ok {
        if is_conflict(&out) {
            *result.lock().unwrap() = Some(GitTaskStatus::Conflict);
            return format!("⚠ Conflict pulling main!\n{}\n\nOpen Fork to resolve.", out.trim());
        }
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] pull main: {}", out);
    }

    check!(); prog!(0.55);
    upd!(format!("[3/4] Merging {} → main…", from_branch));
    let (ok, out) = run_git(&dir, &["merge", &from_branch]);
    if !ok {
        if is_conflict(&out) {
            *result.lock().unwrap() = Some(GitTaskStatus::Conflict);
            return format!(
                "⚠ Merge conflict: {} → main\n{}\n\nOpen Fork to resolve the conflicts.",
                from_branch, out.trim()
            );
        }
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] merge: {}", out);
    }

    prog!(0.80);
    upd!("[4/4] Pushing main…  (no force)");
    let (ok, out) = run_git(&dir, &["push", "origin", "main"]);
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] push main: {}\n⚠ Force push is never allowed.", out);
    }

    prog!(1.0);
    *result.lock().unwrap() = Some(GitTaskStatus::Ok);
    format!("[DONE] Merged {} → main and pushed.", from_branch)
}

pub fn task_git_checkout(
    dir:      PathBuf,
    branch:   String,
    status:   Arc<Mutex<String>>,
    result:   Arc<Mutex<Option<GitTaskStatus>>>,
    cancel:   Arc<AtomicBool>,
    progress: Arc<Mutex<f32>>,
) -> String {
    if cancel.load(Ordering::Relaxed) {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return "[CANCELLED] Git operation was cancelled.".to_string();
    }
    *progress.lock().unwrap() = 0.3;
    *status.lock().unwrap() = format!("Switching to {}…", branch);
    let (ok, out) = run_git(&dir, &["checkout", &branch]);
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] checkout: {}", out);
    }
    *progress.lock().unwrap() = 1.0;
    *result.lock().unwrap() = Some(GitTaskStatus::Ok);
    format!("[DONE] Switched to  {}", branch)
}

pub fn task_git_create_branch(
    dir:      PathBuf,
    name:     String,
    status:   Arc<Mutex<String>>,
    result:   Arc<Mutex<Option<GitTaskStatus>>>,
    cancel:   Arc<AtomicBool>,
    progress: Arc<Mutex<f32>>,
) -> String {
    if cancel.load(Ordering::Relaxed) {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return "[CANCELLED] Git operation was cancelled.".to_string();
    }
    *progress.lock().unwrap() = 0.3;
    *status.lock().unwrap() = format!("Creating branch {}…", name);
    let (ok, out) = run_git(&dir, &["checkout", "-b", &name]);
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] create branch: {}", out);
    }
    *progress.lock().unwrap() = 1.0;
    *result.lock().unwrap() = Some(GitTaskStatus::Ok);
    format!("[DONE] Created and switched to  {}", name)
}
