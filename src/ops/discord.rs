/// Opens Discord on the PC, restores it from minimised/tray, searches for the
/// username with Ctrl+K, and presses Enter to jump straight into the chat.
pub fn open_discord_dm(username: &str) {
    let escaped = escape_sendkeys(username.trim());
    if escaped.is_empty() { return; }

    let ps1_path = std::env::temp_dir().join("devtool_discord_dm.ps1");

    // Single PowerShell script that does everything:
    //   1. Win32 ShowWindow/SetForegroundWindow to restore a minimised window
    //   2. WScript.Shell COM for AppActivate + SendKeys (same API as VBScript)
    // Run via std::process::Command directly (no CREATE_NO_WINDOW) so the
    // process lives on the interactive desktop and can interact with other windows.
    // -WindowStyle Hidden keeps the console invisible.
    let ps1 = format!(
r#"$wsh = New-Object -ComObject WScript.Shell

$sig = @'
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
'@
Add-Type -MemberDefinition $sig -Name Win32 -Namespace WH -ErrorAction SilentlyContinue

function Restore-Discord {{
    $p = Get-Process discord -ErrorAction SilentlyContinue |
         Where-Object {{ $_.MainWindowHandle -ne 0 }} |
         Select-Object -First 1
    if ($p) {{
        [WH.Win32]::ShowWindow($p.MainWindowHandle, 9) | Out-Null
        Start-Sleep -Milliseconds 300
        [WH.Win32]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
        return $true
    }}
    return $false
}}

if (-not (Restore-Discord)) {{
    Start-Process "cmd" -ArgumentList "/c start discord:"
    Start-Sleep -Seconds 3
    Restore-Discord | Out-Null
}}

Start-Sleep -Milliseconds 600

for ($i = 0; $i -lt 10; $i++) {{
    if ($wsh.AppActivate("Discord")) {{ break }}
    Start-Sleep -Milliseconds 400
}}

Start-Sleep -Milliseconds 600

$wsh.SendKeys("{{ESC}}")
Start-Sleep -Milliseconds 300
$wsh.SendKeys("^k")
Start-Sleep -Milliseconds 900
$wsh.SendKeys("{name}")
Start-Sleep -Milliseconds 1200
$wsh.SendKeys("{{ENTER}}")
"#,
        name = escaped,
    );

    if std::fs::write(&ps1_path, ps1.as_bytes()).is_ok() {
        let _ = crate::ops::cmd("powershell")
            .args([
                "-ExecutionPolicy", "Bypass",
                "-WindowStyle",     "Hidden",
                "-NonInteractive",
                "-File",            &ps1_path.to_string_lossy(),
            ])
            .spawn();
    }
}

/// Escapes WScript.Shell SendKeys special chars so they are typed literally.
fn escape_sendkeys(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        '+' | '^' | '%' | '~' | '(' | ')' | '[' | ']' | '{' | '}' => {
            vec!['{', c, '}']
        }
        _ => vec![c],
    }).collect()
}
