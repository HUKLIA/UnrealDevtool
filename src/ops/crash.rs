//! Reading Unreal's crash reports.
//!
//! When the editor or a game crashes, Unreal leaves a folder under
//! `Saved/Crashes` with a `CrashContext.runtime-xml`: what kind of crash it was,
//! the error message, the engine build and the call stack. This pulls the
//! useful lines out so the reason is on screen instead of in a file most people
//! never open. It is a plain-text read; nothing is uploaded anywhere.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Default, PartialEq, Debug)]
pub struct CrashInfo {
    pub dir:        PathBuf,
    pub folder:     String,
    pub age_secs:   u64,
    /// `Crash`, `Ensure`, `Assert`, `GPUCrash`… as Unreal names it.
    pub kind:       String,
    pub message:    String,
    pub executable: String,
    pub engine:     String,
    /// The first frames of the crashing call stack.
    pub stack:      Vec<String>,
}

/// How many stack frames to keep.
const STACK_FRAMES: usize = 10;

/// The text between `<name>` and `</name>`, with XML entities decoded.
fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    Some(unescape(xml[start..end].trim()))
}

fn unescape(s: &str) -> String {
    // The CDATA wrapper Unreal sometimes uses for messages.
    let s = s.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")).unwrap_or(s);
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"")
        .replace("&apos;", "'").replace("&#10;", "\n").replace("&#13;", "").replace("&amp;", "&")
}

/// Parses the fields worth showing from a `CrashContext.runtime-xml`.
pub fn parse_context(xml: &str) -> CrashInfo {
    let stack_text = tag(xml, "CallStack").unwrap_or_default();
    // One frame per line when there are line breaks, otherwise the module names
    // come space-separated on one line.
    let frames: Vec<String> = if stack_text.contains('\n') {
        stack_text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
    } else {
        stack_text.split_whitespace().map(str::to_string).collect()
    };
    CrashInfo {
        kind:       tag(xml, "CrashType").unwrap_or_default(),
        message:    tag(xml, "ErrorMessage").unwrap_or_default(),
        executable: tag(xml, "ExecutableName").unwrap_or_default(),
        engine:     tag(xml, "EngineVersion").or_else(|| tag(xml, "BuildVersion")).unwrap_or_default(),
        stack:      frames.into_iter().take(STACK_FRAMES).collect(),
        ..Default::default()
    }
}

/// The newest crash under `<project>/Saved/Crashes`, read.
pub fn latest(project_dir: &Path) -> Option<CrashInfo> {
    let root = project_dir.join("Saved").join("Crashes");
    let (modified, dir) = std::fs::read_dir(&root).ok()?.flatten()
        .filter_map(|e| {
            let md = e.metadata().ok()?;
            if !md.is_dir() { return None; }
            Some((md.modified().ok()?, e.path()))
        })
        .max_by_key(|(t, _)| *t)?;

    let xml = std::fs::read_to_string(dir.join("CrashContext.runtime-xml")).unwrap_or_default();
    let mut info = parse_context(&xml);
    info.folder = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
    info.age_secs = SystemTime::now().duration_since(modified).map(|d| d.as_secs()).unwrap_or(0);
    info.dir = dir;
    Some(info)
}

impl CrashInfo {
    /// Text for a bug report or a search.
    pub fn report(&self) -> String {
        let mut r = format!("{} in {} ({})\n", if self.kind.is_empty() { "Crash" } else { &self.kind },
            if self.executable.is_empty() { "Unreal" } else { &self.executable }, self.engine);
        if !self.message.is_empty() { r.push_str(&format!("{}\n", self.message)); }
        if !self.stack.is_empty() {
            r.push_str("\nCall stack:\n");
            for f in &self.stack { r.push_str(&format!("  {f}\n")); }
        }
        r
    }

    /// The error line trimmed to something searchable: the part before the
    /// first address or path, which is what other people's reports share.
    pub fn search_terms(&self) -> String {
        let msg = self.message.lines().next().unwrap_or("");
        let cut = msg.find(" 0x").or_else(|| msg.find(" at ")).unwrap_or(msg.len());
        let head: String = msg[..cut].chars().take(120).collect();
        format!("unreal engine {head}").trim().to_string()
    }
}

/// Percent-encodes a query for a URL.
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = "<?xml version=\"1.0\"?><FGenericCrashContext><RuntimeProperties>\
        <CrashVersion>3</CrashVersion><CrashType>Crash</CrashType>\
        <ErrorMessage>Unhandled Exception: EXCEPTION_ACCESS_VIOLATION reading address 0x0000000000000138 &amp; more</ErrorMessage>\
        <ExecutableName>UE4Editor</ExecutableName><EngineVersion>5.7.0-1+++UE5+Release-5.7</EngineVersion>\
        <CallStack>UE4Editor_CoreUObject UE4Editor_Engine UE4Editor kernel32 ntdll</CallStack>\
        </RuntimeProperties></FGenericCrashContext>";

    #[test]
    fn a_crash_context_is_parsed() {
        let c = parse_context(XML);
        assert_eq!(c.kind, "Crash");
        assert!(c.message.starts_with("Unhandled Exception: EXCEPTION_ACCESS_VIOLATION") && c.message.ends_with("& more"));
        assert_eq!((c.executable.as_str(), c.engine.as_str()), ("UE4Editor", "5.7.0-1+++UE5+Release-5.7"));
        assert_eq!(c.stack.len(), 5);
        assert_eq!(c.stack[0], "UE4Editor_CoreUObject");
    }

    #[test]
    fn multiline_stacks_are_split_by_line_and_capped() {
        let frames: String = (0..30).map(|i| format!("Module{i} 0x00007ff + {i:x}\n")).collect();
        let c = parse_context(&format!("<CallStack>{frames}</CallStack>"));
        assert_eq!(c.stack.len(), STACK_FRAMES);
        assert_eq!(c.stack[1], "Module1 0x00007ff + 1");
    }

    #[test]
    fn missing_fields_are_empty_not_errors() {
        let c = parse_context("not xml at all");
        assert_eq!(c, CrashInfo::default());
    }

    #[test]
    fn the_newest_crash_folder_is_found_and_searchable() {
        let root = std::env::temp_dir().join(format!("udt-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("Saved/Crashes/UECC-Windows-ABC_0000");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("CrashContext.runtime-xml"), XML).unwrap();
        let c = latest(&root).expect("a crash");
        assert_eq!(c.folder, "UECC-Windows-ABC_0000");
        assert!(c.age_secs < 60);
        assert_eq!(c.search_terms(), "unreal engine Unhandled Exception: EXCEPTION_ACCESS_VIOLATION reading address");
        assert!(c.report().contains("Call stack:"));
        assert!(latest(&root.join("nowhere")).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn queries_are_percent_encoded() {
        assert_eq!(url_encode("unreal engine: A&B"), "unreal+engine%3A+A%26B");
    }
}
