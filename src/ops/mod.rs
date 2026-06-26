pub mod discord;
pub mod git;
pub mod package;
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
