//! Discord hand-off.
//!
//! This used to write a PowerShell script to `%TEMP%` and run it hidden with
//! `-ExecutionPolicy Bypass`, using `WScript.Shell.SendKeys` to type a username
//! and a message into whatever window Discord had. Antivirus engines treat that
//! exact combination — a dropped script, a hidden interpreter with the policy
//! bypassed, and synthetic keystrokes into another application — as a
//! keylogger/RAT shape, and it was one of the strongest reasons this app was
//! flagged.
//!
//! It now only asks Windows to open Discord through its own URL handler. The
//! message goes to the clipboard (done by the caller) and the person pastes it,
//! which is also more reliable than guessing at Discord's focus timing.

/// Opens the Discord desktop app (or the web client if the protocol handler is
/// not installed) at the direct-messages screen.
pub fn open_discord() {
    let _ = crate::ops::cmd("explorer").arg("discord://-/channels/@me").spawn();
}

/// Shows a file selected in Explorer so it can be dragged into Discord.
pub fn reveal_in_explorer(path: &str) {
    let path = path.trim();
    if path.is_empty() { return; }
    let _ = crate::ops::cmd("explorer").arg(format!("/select,{path}")).spawn();
}
