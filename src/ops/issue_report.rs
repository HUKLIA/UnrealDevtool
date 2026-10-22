//! Local, editable issue reports assembled from build history or a playtest receipt.
//! Log paths are opt-in attachments; this module never reads log contents.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogAttachment {
    pub label: String,
    pub path: PathBuf,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueReportDraft {
    pub project: String,
    pub build_identity: String,
    pub source: String,
    pub requested_status: String,
    pub confirmed_status: String,
    pub expected_behavior: String,
    pub actual_behavior: String,
    pub reproduction_steps: String,
    pub attachments: Vec<LogAttachment>,
}

impl IssueReportDraft {
    pub fn from_build(project_file: &Path, build: &crate::types::BuildOutcome, project_log: PathBuf) -> Self {
        let state = match build.outcome {
            crate::types::PackageOutcome::Succeeded => "succeeded",
            crate::types::PackageOutcome::Failed => "failed",
            crate::types::PackageOutcome::Cancelled => "cancelled",
        };
        let mut attachments = Vec::new();
        if let Some(path) = build.log.as_ref() {
            attachments.push(LogAttachment { label: "Build log (path only; not copied)".into(), path: path.clone(), selected: false });
        }
        attachments.push(LogAttachment { label: "Project log (path only; not copied)".into(), path: project_log, selected: false });
        Self {
            project: project_name(project_file),
            build_identity: format!("{} · {} · {}", build.version, build.platform.label(), build.config.as_str()),
            source: "Package run".into(),
            requested_status: format!("Packaging operation reported {state}."),
            confirmed_status: if build.is_success() {
                "Local packaging completed; game startup and runtime behavior are not confirmed.".into()
            } else if build.outcome == crate::types::PackageOutcome::Cancelled {
                "Packaging was cancelled; no game runtime result is available.".into()
            } else {
                "Packaging did not complete successfully; game runtime behavior is not confirmed.".into()
            },
            expected_behavior: String::new(),
            actual_behavior: "Not recorded.".into(),
            reproduction_steps: String::new(),
            attachments,
        }
    }

    pub fn from_history(project_file: &Path, record: &crate::ops::history::BuildRecord, project_log: PathBuf) -> Self {
        let info = record.info.as_ref();
        let outcome = info.and_then(|i| i.outcome).or_else(|| info.map(|i| if i.ok {
            crate::types::PackageOutcome::Succeeded
        } else { crate::types::PackageOutcome::Failed }));
        let requested_status = match outcome {
            Some(crate::types::PackageOutcome::Succeeded) => "Build history records success.".into(),
            Some(crate::types::PackageOutcome::Failed) => "Build history records failure.".into(),
            Some(crate::types::PackageOutcome::Cancelled) => "Build history records cancellation.".into(),
            None => "Build status was not recorded.".into(),
        };
        let confirmed_status = match outcome {
            Some(crate::types::PackageOutcome::Succeeded) => "Package operation reported success; game runtime behavior is not confirmed.".into(),
            Some(crate::types::PackageOutcome::Failed) => "Package operation reported failure; game runtime behavior is not confirmed.".into(),
            Some(crate::types::PackageOutcome::Cancelled) => "Package operation was cancelled; no game runtime result is available.".into(),
            None => "Not confirmed: this history record has no typed result.".into(),
        };
        let mut attachments = Vec::new();
        let build_log = record.dir.join("BuildLog.txt");
        if build_log.is_file() {
            attachments.push(LogAttachment { label: "Build log (path only; not copied)".into(), path: build_log, selected: false });
        }
        attachments.push(LogAttachment { label: "Project log (path only; not copied)".into(), path: project_log, selected: false });
        Self {
            project: project_name(project_file),
            build_identity: format!("{} · {} · {}", record.version,
                info.map(|i| i.platform.as_str()).filter(|s| !s.is_empty()).unwrap_or("Platform not recorded"),
                info.map(|i| i.config.as_str()).filter(|s| !s.is_empty()).unwrap_or("Configuration not recorded")),
            source: "Saved build history".into(), requested_status, confirmed_status,
            expected_behavior: String::new(), actual_behavior: "Not recorded.".into(),
            reproduction_steps: String::new(), attachments,
        }
    }

    pub fn from_playtest(project_file: &Path, receipt: &str, build_identity: Option<String>) -> Self {
        let mode = receipt_value(receipt, "Mode:").unwrap_or_else(|| "Not recorded".into());
        let (requested_status, confirmed_status) = if receipt.contains("Windows accepted the launch request") {
            (format!("Launch requested for {mode}; Windows accepted the request."),
             "Not confirmed: the receipt does not verify game startup or completion.".into())
        } else if receipt.contains("Launch request failed") {
            (format!("Launch request for {mode} failed."),
             "Confirmed only that Windows rejected the launch request; game behavior is unavailable.".into())
        } else {
            (format!("Launch attempt recorded for {mode}; request status is unavailable."),
             "Not confirmed: the receipt contains no verified runtime result.".into())
        };
        let project_log = receipt_value(receipt, "Project log (if written):").map(PathBuf::from);
        let attachments = project_log.into_iter().map(|path| LogAttachment {
            label: "Project log (path only; not copied)".into(), path, selected: false,
        }).collect();
        Self {
            project: project_name(project_file),
            build_identity: build_identity.unwrap_or_else(|| "Not recorded".into()),
            source: "Playtest launch receipt".into(),
            requested_status,
            confirmed_status,
            expected_behavior: String::new(),
            actual_behavior: "Not recorded.".into(),
            reproduction_steps: String::new(),
            attachments,
        }
    }

    pub fn markdown(&self) -> String {
        let selected: Vec<_> = self.attachments.iter().filter(|a| a.selected).collect();
        let attachments = if selected.is_empty() { "None selected.".to_string() }
            else { selected.iter().map(|a| format!("- {}", markdown_code_span(&a.path.display().to_string()))).collect::<Vec<_>>().join("\n") };
        format!(
            "# Unreal issue report\n\n- Project: {}\n- Build/run: {}\n- Source: {}\n- Requested status: {}\n- Confirmed status: {}\n\n## Expected behavior\n{}\n\n## Actual behavior\n{}\n\n## Reproduction steps\n{}\n\n## Selected log paths\n{}\n",
            markdown_escape(&self.project), markdown_escape(&self.build_identity), markdown_escape(&self.source),
            markdown_escape(&self.requested_status), markdown_escape(&self.confirmed_status),
            nonempty(&self.expected_behavior), nonempty(&self.actual_behavior),
            nonempty(&self.reproduction_steps), attachments,
        )
    }

    pub fn export_to(&self, path: &Path) -> Result<(), String> {
        if path.extension().is_none_or(|ext| !ext.eq_ignore_ascii_case("md")) {
            return Err("Choose a Markdown (.md) file for the local report export.".into());
        }
        let path_text = path.to_string_lossy();
        if path_text.starts_with(r"\\") || path_text.starts_with("//") {
            return Err("Choose a local disk path; network-share exports are not supported.".into());
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    format!("Report already exists at {}. Choose a new file name to preserve it.", path.display())
                } else {
                    format!("Could not create report at {}: {e}", path.display())
                }
            })?;
        if let Err(e) = file.write_all(self.markdown().as_bytes()) {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(format!("Could not finish report export to {}: {e}", path.display()));
        }
        Ok(())
    }

    pub fn suggested_filename(&self) -> String {
        let safe: String = self.project.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        format!("Unreal-Issue-{}.md", if safe.is_empty() { "Report" } else { &safe })
    }
}

fn receipt_value(receipt: &str, key: &str) -> Option<String> {
    receipt.lines().find_map(|line| line.strip_prefix(key).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string))
}

fn project_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Not recorded".into())
}

fn nonempty(value: &str) -> &str {
    if value.trim().is_empty() { "Not recorded." } else { value }
}

fn markdown_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('|', "\\|").replace('*', "\\*")
        .replace('_', "\\_").replace('[', "\\[").replace(']', "\\]")
        .replace('`', "\\`").replace(['\r', '\n'], " ")
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

    #[test]
    fn launch_report_does_not_claim_runtime_confirmation_or_copy_arguments() {
        let receipt = "Mode: Packaged game\nArguments: -token=PRIVATE\nResult: Windows accepted the launch request. This does not confirm the game started or completed.\nProject log (if written): C:/private/Saved/Logs/Game.log";
        let draft = IssueReportDraft::from_playtest(Path::new("Game.uproject"), receipt, None);
        let markdown = draft.markdown();
        assert!(draft.confirmed_status.starts_with("Not confirmed"));
        assert!(!markdown.contains("PRIVATE"));
        assert!(markdown.contains("Not recorded."));
        assert!(markdown.contains("None selected."));
        assert!(!draft.attachments[0].selected);
    }

    #[test]
    fn draft_serialization_preserves_edits_and_attachment_choice() {
        let mut draft = IssueReportDraft::from_playtest(Path::new("Game.uproject"), "Mode: Editor\nResult: unknown", Some("v1 · Windows · Development".into()));
        draft.expected_behavior = "Map loads".into();
        draft.actual_behavior = "Black screen".into();
        draft.reproduction_steps = "1. Launch\n2. Open map".into();
        draft.attachments.push(LogAttachment { label: "Build log".into(), path: PathBuf::from("BuildLog.txt"), selected: true });
        let json = serde_json::to_string(&draft).unwrap();
        assert_eq!(serde_json::from_str::<IssueReportDraft>(&json).unwrap(), draft);
    }

    #[test]
    fn saved_cancellation_becomes_a_report_without_claiming_runtime_failure() {
        let record = crate::ops::history::BuildRecord {
            version: "v1.2.3".into(), number: 3, bytes: 0, modified: None,
            dir: PathBuf::from("build/v1.2.3"),
            info: Some(crate::ops::history::BuildInfo {
                platform: "Windows".into(), config: "Development".into(), ok: false,
                outcome: Some(crate::types::PackageOutcome::Cancelled),
                ..crate::ops::history::BuildInfo::default()
            }),
        };
        let draft = IssueReportDraft::from_history(Path::new("Game.uproject"), &record, PathBuf::from("Game.log"));
        assert!(draft.build_identity.contains("v1.2.3"));
        assert!(draft.requested_status.contains("cancellation"));
        assert!(draft.confirmed_status.contains("no game runtime result"));
        assert!(draft.attachments.iter().all(|a| !a.selected));
    }

    #[test]
    fn export_writes_only_selected_log_paths_and_reports_errors() {
        let root = std::env::temp_dir().join(format!("udt-issue-report-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut draft = IssueReportDraft::from_playtest(Path::new("Game.uproject"), "Mode: Editor", None);
        draft.attachments.push(LogAttachment { label: "Build log".into(), path: PathBuf::from("BuildLog.txt"), selected: true });
        let path = root.join("issue.md");
        draft.export_to(&path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("`BuildLog.txt`"));
        assert!(!written.contains("Arguments:"));
        assert!(draft.export_to(&path).unwrap_err().contains("already exists"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), written, "a report export must not overwrite an existing file");
        assert!(draft.export_to(&root).is_err());
        assert!(draft.export_to(&root.join("issue.txt")).is_err());
        assert!(draft.export_to(Path::new(r"\\server\share\issue.md")).unwrap_err().contains("local disk"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn attachment_paths_with_backticks_are_valid_markdown_code_spans() {
        let mut draft = IssueReportDraft::from_playtest(Path::new("Game.uproject"), "Mode: Editor", None);
        draft.attachments.push(LogAttachment { label: "Build log".into(), path: PathBuf::from("C:/Logs/Build`Review.log"), selected: true });
        assert!(draft.markdown().contains("``C:/Logs/Build`Review.log``"));
    }
}
