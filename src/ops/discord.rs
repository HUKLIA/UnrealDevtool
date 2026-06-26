/// Opens Discord on the PC, restores it from minimised/tray, searches for the
/// username with Ctrl+K, and presses Enter to jump straight into the chat.
pub fn open_discord_dm(username: &str) {
    let escaped = escape_sendkeys(username.trim());
    if escaped.is_empty() { return; }

    let tmp       = std::env::temp_dir();
    let ps1_path  = tmp.join("devtool_discord_restore.ps1");
    let vbs_path  = tmp.join("devtool_discord_dm.vbs");
    let ps1_str   = ps1_path.to_string_lossy().to_string();

    // PowerShell: use Win32 ShowWindow(SW_RESTORE) + SetForegroundWindow so the
    // window actually surfaces even when minimised to taskbar or sitting in tray.
    let ps1 = r#"
$sig = @'
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
'@
Add-Type -MemberDefinition $sig -Name Win32 -Namespace WH -ErrorAction SilentlyContinue

function Restore-Discord {
    $p = Get-Process discord -ErrorAction SilentlyContinue |
         Where-Object { $_.MainWindowHandle -ne 0 } |
         Select-Object -First 1
    if ($p) {
        [WH.Win32]::ShowWindow($p.MainWindowHandle, 9) | Out-Null   # SW_RESTORE
        Start-Sleep -Milliseconds 300
        [WH.Win32]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
        return $true
    }
    return $false
}

# If Discord is in tray (no visible window), open it via protocol first
if (-not (Restore-Discord)) {
    Start-Process "cmd" -ArgumentList "/c start discord:"
    Start-Sleep -Seconds 3
    Restore-Discord | Out-Null
}
"#;

    // VBScript: runs the PS1 (waits for it), then AppActivate + SendKeys.
    // Using wscript.exe so the script runs as a desktop GUI process — required
    // for AppActivate and SendKeys to reach the correct window.
    let vbs = format!(r#"Set wsh = CreateObject("WScript.Shell")

' Step 1 - restore/focus Discord via Win32 (handles minimised, tray, not running)
wsh.Run "powershell -ExecutionPolicy Bypass -WindowStyle Hidden -File ""{ps1}""", 0, True

WScript.Sleep 400

' Step 2 - make sure Discord is the foreground window (retry up to 10x)
Dim i
For i = 1 To 10
    If wsh.AppActivate("Discord") Then Exit For
    WScript.Sleep 400
Next

WScript.Sleep 500

' Step 3 - clear any open panel, open DM search, type name, Enter to open chat
wsh.SendKeys "{{ESC}}"
WScript.Sleep 250
wsh.SendKeys "^k"
WScript.Sleep 900
wsh.SendKeys "{name}"
WScript.Sleep 1200
wsh.SendKeys "{{ENTER}}"
"#,
        ps1  = ps1_str,
        name = escaped,
    );

    if std::fs::write(&ps1_path, ps1).is_ok() &&
       std::fs::write(&vbs_path, &vbs).is_ok()
    {
        let _ = std::process::Command::new("wscript")
            .arg(&vbs_path)
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
