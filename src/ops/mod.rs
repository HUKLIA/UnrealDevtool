pub mod cheatsheet;
pub mod adb;
pub mod clean;
pub mod crash;
pub mod clock;
pub mod diagnostics;
pub mod engines;
pub mod editor_repair;
pub mod doctor;
pub mod discord;
pub mod git;
pub mod history;
pub mod handoff;
pub mod insights;
pub mod issue_report;
pub mod llm;
pub mod monitor;
pub mod package;
pub mod plugins;
pub mod preflight;
pub mod rclone;
pub mod run;
pub mod selfcheck;
pub mod tools;
pub mod update;
pub mod vs;

use std::path::Path;
use std::process::Command;

/// Returns a `Command` with `CREATE_NO_WINDOW` set so no black console popup
/// appears when the GUI app spawns child processes on Windows.
pub fn cmd(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Creates a command that invokes a Windows batch file and preserves paths
/// containing spaces. `call` is important here: without it, `cmd /c` can
/// parse a quoted batch-file path as the whole command and drop the remaining
/// arguments on some Windows versions.
pub fn batch_cmd(script: &Path) -> Command {
    let mut command = cmd("cmd");
    command.args(["/d", "/c", "call"]).arg(script);
    command
}

/// Opens `url` in the user's default browser via the OS URI handler.
/// Fire-and-forget: there's nothing actionable to do on the UI thread if no
/// default browser is registered, so failures are swallowed.
pub fn open_url(url: &str) {
    if crate::config::offline_mode() { return; }
    if !is_safe_url(url) { return; }
    let _ = cmd("cmd").args(["/c", "start", "", url]).spawn();
}

/// True if `url` can be handed to `cmd /c start` without cmd reinterpreting
/// it. Custom links come from the user's config file, so a value like
/// `https://x & evil.exe` must never reach cmd's parser. Only web links and
/// Windows Settings pages are allowed, and any cmd metacharacter is refused.
pub fn is_safe_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let scheme_ok = lower.starts_with("https://") || lower.starts_with("http://")
        || lower.starts_with("ms-settings:");
    // `%` is allowed only as percent-encoding (`%20`), never as `%VAR%`.
    let bytes = url.as_bytes();
    let percent_ok = bytes.iter().enumerate().all(|(i, &b)| b != b'%'
        || (bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
            && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit)));
    scheme_ok && percent_ok && !url.chars().any(|c| c.is_whitespace() || c.is_control()
        || matches!(c, '&' | '|' | '<' | '>' | '^' | '"' | '!' | '(' | ')' | '`'))
}

#[cfg(test)]
mod url_tests {
    use super::is_safe_url;
    #[test]
    fn only_plain_web_and_settings_links_reach_cmd() {
        assert!(is_safe_url("https://github.com/HUKLIA/UnrealDevtool/releases/latest"));
        assert!(is_safe_url("ms-settings:defaultapps"));
        assert!(is_safe_url("https://example.com/search?q=a=b#x"));
        assert!(!is_safe_url("https://x.com & calc.exe"));
        assert!(!is_safe_url("https://x.com&calc"));
        assert!(!is_safe_url("https://x.com|calc"));
        assert!(!is_safe_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_safe_url("C:\\evil.bat"));
        assert!(!is_safe_url("https://x.com/%PATH%"));
        assert!(is_safe_url("https://x.com/a%20b"));
    }
}
