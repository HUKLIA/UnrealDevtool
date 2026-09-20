pub mod clean;
pub mod clock;
pub mod diagnostics;
pub mod doctor;
pub mod discord;
pub mod git;
pub mod history;
pub mod llm;
pub mod monitor;
pub mod package;
pub mod preflight;
pub mod rclone;
pub mod run;
pub mod selfcheck;
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
    let _ = cmd("cmd").args(["/c", "start", "", url]).spawn();
}
