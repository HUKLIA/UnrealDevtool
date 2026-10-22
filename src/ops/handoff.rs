//! Small, local handoff notes for a successfully packaged artifact.
//!
//! Call `write_verified_archive_handoff` only after the packaging pipeline has
//! verified the ZIP's directory and entries. This records the exact archive
//! bytes for a QA/release handoff; it does not claim the game was launched or
//! tested.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub const HANDOFF_FILE: &str = "build-handoff.md";

pub fn write_verified_archive_handoff(
    archive_path: &Path,
    project_name: &str,
    version: &str,
    platform: &str,
    configuration: &str,
    method: &str,
) -> Result<PathBuf, String> {
    let metadata = fs::metadata(archive_path)
        .map_err(|e| format!("Could not inspect packaged archive: {e}"))?;
    if !metadata.is_file() {
        return Err("Packaged archive path is not a file.".into());
    }
    let (sha256, bytes) = sha256_file(archive_path)
        .map_err(|e| format!("Could not hash packaged archive: {e}"))?;
    let name = archive_path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| "Packaged archive has no file name.".to_string())?;
    let text = render_handoff(project_name, version, platform, configuration, method,
        &name, bytes, &sha256);
    let folder = archive_path.parent()
        .ok_or_else(|| "Packaged archive has no output folder.".to_string())?;
    let path = folder.join(HANDOFF_FILE);

    let mut file = OpenOptions::new().write(true).create_new(true).open(&path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!("Handoff file already exists at {}. It was left unchanged.", path.display())
            } else {
                format!("Could not create build handoff at {}: {e}", path.display())
            }
        })?;
    if let Err(e) = file.write_all(text.as_bytes()) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(format!("Could not finish build handoff at {}: {e}", path.display()));
    }
    Ok(path)
}

fn sha256_file(path: &Path) -> std::io::Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 { break; }
        hash.update(&buffer[..read]);
        bytes = bytes.saturating_add(read as u64);
    }
    Ok((format!("{:x}", hash.finalize()), bytes))
}

#[allow(clippy::too_many_arguments)]
fn render_handoff(
    project_name: &str,
    version: &str,
    platform: &str,
    configuration: &str,
    method: &str,
    archive_name: &str,
    bytes: u64,
    sha256: &str,
) -> String {
    format!(
        "# Build handoff: {} {}\n\n\
- Platform: {}\n- Configuration: {}\n- Build method: {}\n- Archive: {}\n- Archive size: {} bytes\n- SHA-256: `{}`\n\n\
The archive passed Unreal DevTool's ZIP integrity check when this handoff was created. \
Compare the SHA-256 after transferring the archive to confirm it is the same file.\n\n\
This verifies archive integrity only. It does not confirm extraction, installation, game startup, \
visuals, audio, gameplay, or performance.\n\n\
## Tester checklist\n\n\
- [ ] Confirm the transferred archive's SHA-256 matches above\n- [ ] Extract or install it on the stated platform\n\
- [ ] Run the agreed test scenario and record expected versus actual behavior\n- [ ] File an issue report for anything that needs follow-up\n",
        markdown_escape(project_name), markdown_escape(version), markdown_escape(platform),
        markdown_escape(configuration), markdown_escape(method), markdown_code_span(archive_name),
        bytes, sha256,
    )
}

fn markdown_escape(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
        .replace('|', "\\|").replace('*', "\\*").replace('_', "\\_")
        .replace('[', "\\[").replace(']', "\\]").replace('`', "\\`")
}

fn markdown_code_span(value: &str) -> String {
    let max_ticks = value.split('`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(max_ticks + 1);
    let padding = if value.starts_with(' ') || value.starts_with('`') || value.ends_with(' ') || value.ends_with('`') { " " } else { "" };
    format!("{fence}{padding}{value}{padding}{fence}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("udt-handoff-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn handoff_hashes_the_archive_and_includes_a_honest_test_checklist() {
        let root = temp_dir();
        let archive = root.join("Game_v1.zip");
        fs::write(&archive, b"abc").unwrap();
        let path = write_verified_archive_handoff(&archive, "Game", "v1", "Windows", "Shipping", "Full package").unwrap();
        let report = fs::read_to_string(path).unwrap();
        assert!(report.contains("SHA-256: `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`"));
        assert!(report.contains("Archive size: 3 bytes"));
        assert!(report.contains("does not confirm extraction, installation, game startup"));
        assert!(report.contains("Confirm the transferred archive's SHA-256"));
        assert!(!report.contains(root.to_string_lossy().as_ref()), "handoff must not disclose local paths");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn existing_handoff_is_not_overwritten() {
        let root = temp_dir();
        let archive = root.join("Game.zip");
        fs::write(&archive, b"archive").unwrap();
        let handoff = root.join(HANDOFF_FILE);
        fs::write(&handoff, "preserve this").unwrap();
        assert!(write_verified_archive_handoff(&archive, "Game", "v1", "Windows", "Development", "Full package")
            .unwrap_err().contains("left unchanged"));
        assert_eq!(fs::read_to_string(handoff).unwrap(), "preserve this");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn markdown_fields_escape_markup_and_archive_names_with_ticks() {
        let rendered = render_handoff("Game|one", "v1", "Win*64", "Shipping", "Full", "weird`file.zip", 42, "abc");
        assert!(rendered.contains("Game\\|one"));
        assert!(rendered.contains("Win\\*64"));
        assert!(rendered.contains("``weird`file.zip``"));
    }
}
