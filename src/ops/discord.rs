use std::path::PathBuf;

/// Opens Discord on the PC, focuses it, presses Ctrl+K, then types the username.
/// Writes a temp VBScript and runs it via wscript.exe — the only reliable way to
/// do desktop focus + SendKeys from a background process on Windows.
pub fn open_discord_dm(username: &str) {
    let escaped = escape_sendkeys(username.trim());
    if escaped.is_empty() { return; }

    // wscript.exe runs as a desktop app so WScript.Shell.AppActivate and
    // SendKeys work correctly. PowerShell with -WindowStyle Hidden sits in
    // a non-interactive session and can't reliably focus other windows.
    let vbs = format!(
r#"Dim wsh
Set wsh = CreateObject("WScript.Shell")

' Open Discord (protocol handler brings it to front if already running)
wsh.Run "cmd /c start discord:", 1, False
WScript.Sleep 2500

' Try to activate by window title; Discord title always starts with "Discord"
Dim activated
activated = wsh.AppActivate("Discord")
If Not activated Then
    WScript.Sleep 1500
    wsh.AppActivate("Discord")
End If
WScript.Sleep 600

' Ctrl+K opens the quick-switcher / DM search in Discord
wsh.SendKeys "^k"
WScript.Sleep 700

' Type the username
wsh.SendKeys "{name}"
"#,
        name = escaped
    );

    let tmp: PathBuf = std::env::temp_dir().join("devtool_discord_dm.vbs");
    if std::fs::write(&tmp, &vbs).is_ok() {
        // wscript.exe is the GUI VBScript host — no console window, full desktop access
        let _ = std::process::Command::new("wscript")
            .arg(&tmp)
            .spawn();
    }
}

/// Escapes WScript.Shell SendKeys special characters so they are typed literally.
/// + ^ % ~ ( ) [ ] { } are key-combo modifiers; wrap each in braces to type it.
fn escape_sendkeys(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        '+' | '^' | '%' | '~' | '(' | ')' | '[' | ']' | '{' | '}' => {
            vec!['{', c, '}']
        }
        _ => vec![c],
    }).collect()
}
