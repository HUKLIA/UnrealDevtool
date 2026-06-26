/// Opens Discord on the PC, focuses it, presses Ctrl+K to open the quick-switcher,
/// then types the target username so Discord searches for that person.
/// Fires and forgets — runs in a hidden PowerShell process so the UI stays responsive.
pub fn open_discord_dm(username: &str) {
    let escaped = escape_sendkeys(username.trim());
    if escaped.is_empty() { return; }

    // PowerShell one-liner:
    //   1. Launch Discord via protocol if not running, wait for it to start.
    //   2. AppActivate to bring the window to foreground.
    //   3. Ctrl+K = quick-switcher / DM search.
    //   4. Type the username.
    let script = format!(
        "$wsh = New-Object -ComObject WScript.Shell; \
         $d = Get-Process discord -ErrorAction SilentlyContinue; \
         if (-not $d) {{ Start-Process 'discord:'; Start-Sleep -Seconds 3 }}; \
         $null = $wsh.AppActivate('Discord'); \
         Start-Sleep -Milliseconds 900; \
         $wsh.SendKeys('^k'); \
         Start-Sleep -Milliseconds 700; \
         $wsh.SendKeys('{}')",
        escaped
    );

    let _ = crate::ops::cmd("powershell")
        .args(["-WindowStyle", "Hidden", "-NonInteractive", "-Command", &script])
        .spawn();
}

/// Escapes special WScript.Shell SendKeys characters so they are typed literally
/// rather than interpreted as key-combo modifiers.
fn escape_sendkeys(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        '+' | '^' | '%' | '~' | '(' | ')' | '[' | ']' | '{' | '}' => {
            vec!['{', c, '}']
        }
        _ => vec![c],
    }).collect()
}
