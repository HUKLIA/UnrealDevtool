use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
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

    check!(); prog!(0.05);
    upd!("[1/2] Fetching origin/main…");
    let (ok, out) = run_git(&dir, &["fetch", "origin", "main"]);
    if !ok {
        *result.lock().unwrap() = Some(GitTaskStatus::Error);
        return format!("[ERROR] fetch: {}", out);
    }

    check!(); prog!(0.50);
    upd!("[2/2] Rebasing onto origin/main…");
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

    prog!(1.0);
    *result.lock().unwrap() = Some(GitTaskStatus::Ok);
    format!("[DONE] Synced with origin/main via rebase.\n{}", out.trim())
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
