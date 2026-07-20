pub mod ads;
pub mod discord;
pub mod git;
pub mod package;
pub mod preflight;
pub mod update;
pub mod vs;

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

/// Opens `url` in the user's default browser via the OS URI handler.
/// Fire-and-forget: there's nothing actionable to do on the UI thread if no
/// default browser is registered, so failures are swallowed.
pub fn open_url(url: &str) {
    let _ = cmd("cmd").args(["/c", "start", "", url]).spawn();
}
