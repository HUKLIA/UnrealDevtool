/// Opens Discord on the PC, restores it from minimised/tray, searches for the
/// username with Ctrl+K, and presses Enter to jump straight into the chat.
/// If `message` is given, types it into the chat box and sends it (Enter).
pub fn open_discord_dm(username: &str, message: Option<&str>, image_path: Option<&str>) {
    let escaped = escape_sendkeys(username.trim());
    if escaped.is_empty() { return; }


    // Extra SendKeys lines appended after the chat is opened, only when a preset/custom message was provided
    let message_block = match message.map(str::trim).filter(|m| !m.is_empty()) {
        Some(msg) => format!(
            "Start-Sleep -Milliseconds 900\n$wsh.SendKeys(\"{}\")\nStart-Sleep -Milliseconds 400\n$wsh.SendKeys(\"{{ENTER}}\")\n",
            escape_sendkeys(msg)
        ),
        None => String::new(),
    };

    let image_block = match image_path.map(str::trim).filter(|m| !m.is_empty()) {Some(path) => format!(
            r#"
    # Copy image file to clipboard then paste into Discord
    Add-Type -AssemblyName System.Windows.Forms
    $col = New-Object System.Collections.Specialized.StringCollection
    $col.Add("{path}")
    [System.Windows.Forms.Clipboard]::SetFileDropList($col)
    Start-Sleep -Milliseconds 700
    $wsh.SendKeys("^v")
    Start-Sleep -Milliseconds 1200
    $wsh.SendKeys("{{ENTER}}")
    "#,
        path = path
    ),
        None => String::new(),
    };


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

Start-Sleep -Milliseconds 400

for ($i = 0; $i -lt 10; $i++) {{
    if ($wsh.AppActivate("Discord")) {{ break }}
    Start-Sleep -Milliseconds 400
}}

Start-Sleep -Milliseconds 500

$wsh.SendKeys("{{ESC}}")
Start-Sleep -Milliseconds 150
$wsh.SendKeys("^k")
Start-Sleep -Milliseconds 250
$wsh.SendKeys("{name}")
Start-Sleep -Milliseconds 500
$wsh.SendKeys("{{ENTER}}")
{message_block}{image_block}"#,

                      name = escaped,
                      message_block = message_block,
                      image_block = image_block,
    );

    if std::fs::write(&ps1_path, ps1.as_bytes()).is_ok() {
        let _ = crate::ops::cmd("powershell")
            .args([
                "-STA",                         // required for SetFileDropList (image clipboard)
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
