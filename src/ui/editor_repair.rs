use crate::app::DevToolApp;
use crate::ops::editor_repair::{self as repair, FileFix};
use crate::theme::*;
use eframe::egui;
use std::path::PathBuf;

enum Preview {
    File(repair::FilePlan),
    Association(repair::AssociationPlan),
    RestoreAssociation(PathBuf, repair::AssociationPlan),
    RestoreFile(repair::FileUndo),
    Launch(PathBuf, Vec<String>),
}

#[derive(Default)]
pub struct RepairState {
    project: Option<PathBuf>,
    engine: Option<PathBuf>,
    inspection: Option<repair::Inspection>,
    selector: Option<PathBuf>,
    preview: Option<Preview>,
    focus_preview: bool,
    undo: Option<repair::FileUndo>,
    association_backup: Option<PathBuf>,
    notice: String,
}

impl RepairState {
    fn set_preview(&mut self, preview: Preview) {
        self.preview = Some(preview);
        self.focus_preview = true;
    }
}

impl DevToolApp {
    pub fn open_editor_repair(&mut self) {
        self.tools.tab = crate::ui::tools::ToolsTab::Repair;
        self.open_sheet(crate::types::Sheet::Tools);
    }

    fn preview_association_restore(&mut self, path: PathBuf) {
        match repair::read_association_backup(&path) {
            Ok(plan) => self
                .tools
                .repair
                .set_preview(Preview::RestoreAssociation(path, plan)),
            Err(e) => self.tools.repair.notice = format!("Could not preview backup: {e}"),
        }
    }

    fn refresh_editor_repair(&mut self) {
        let scan = repair::inspect(self.project_path.as_deref(), self.engine_dir.as_deref());
        let state = &mut self.tools.repair;
        state.project = self.project_path.clone();
        state.engine = self.engine_dir.clone();
        if state
            .selector
            .as_ref()
            .is_none_or(|p| !repair::valid_selector(p))
        {
            state.selector = scan.selector.clone();
        }
        state.inspection = Some(scan);
        state.preview = None;
    }

    pub fn show_editor_repair(&mut self, ui: &mut egui::Ui) {
        if self.tools.repair.inspection.is_none()
            || self.tools.repair.project != self.project_path
            || self.tools.repair.engine != self.engine_dir
        {
            self.tools.repair = RepairState::default();
            self.refresh_editor_repair();
        }
        ui.label(heading("Editor won't open", 20.0));
        ui.label(hint("First separate Windows double-click problems from engine selection and editor startup crashes."));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Scan again").clicked() { self.refresh_editor_repair(); }
            if ui.button("Choose project / engine").clicked() { self.open_sheet(crate::types::Sheet::Diagnostics); }
            if ui.button("Copy diagnosis").clicked()
                && let Some(scan) = &self.tools.repair.inspection {
                    let report = scan.items.iter().map(|i| format!("{}: {}", i.label, i.detail)).collect::<Vec<_>>().join("\n");
                    ui.ctx().copy_text(format!("Unreal DevTool {} editor startup diagnosis\n{report}\nReview local paths before sharing.", env!("CARGO_PKG_VERSION")));
                }
        });
        if let Some(scan) = &self.tools.repair.inspection {
            for item in &scan.items {
                Self::show_check_item(ui, item);
            }
        }
        ui.add_space(10.0);
        card().show(ui, |ui| {
            ui.label(heading("1. Open directly to isolate the problem", 14.0));
            ui.label(hint("This bypasses Windows file associations. If it opens here but double-click fails, repair the Windows association in step 2."));
            ui.label(hint("Choose the intended engine first. Opening with a newer/different engine may convert assets. DX11 is a temporary troubleshooting option, not a universal safe mode."));
            let can_launch = self.project_path.is_some() && self.engine_dir.as_ref().is_some_and(|p| repair::editor_exe(p).is_file()) && !self.is_busy_now();
            ui.add_enabled_ui(can_launch, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, dx11) in [("Preview direct editor launch", false), ("Preview temporary DX11 launch", true)] {
                        if ui.button(label).clicked() {
                            let project = self.project_path.as_ref().unwrap();
                            let exe = repair::editor_exe(self.engine_dir.as_ref().unwrap());
                            self.tools.repair.set_preview(Preview::Launch(exe, repair::launch_args(project, dx11)));
                        }
                    }
                });
            });
        });
        ui.add_space(10.0);
        card().show(ui, |ui| {
            ui.label(heading("2. Repair Windows .uproject opening", 14.0));
            ui.label(hint("For 'no app associated', 'Invalid command line', or missing Unreal right-click commands. Uses the original UnrealVersionSelector in its existing location."));
            ui.label(hint("Registers a handler for this Windows user only. This affects .uproject files for this user; it does not change project EngineAssociation values or require administrator rights."));
            if let Some(path) = &self.tools.repair.selector { ui.monospace(path.display().to_string()); }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Locate UnrealVersionSelector...").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Epic selector", &["exe"]).pick_file() {
                        if repair::valid_selector(&path) { self.tools.repair.selector = Some(path); }
                        else { self.tools.repair.notice = "Choose Epic's original UnrealVersionSelector.exe, not the editor or a copied/downloaded replacement.".into(); }
                    }
                if ui.add_enabled(self.tools.repair.selector.is_some(), egui::Button::new("Preview Windows repair")).clicked() {
                    match repair::plan_association(self.tools.repair.selector.as_ref().unwrap()) {
                        Ok(plan) => self.tools.repair.set_preview(Preview::Association(plan)),
                        Err(e) => self.tools.repair.notice = e,
                    }
                }
                if ui.button("Windows Default apps").clicked() { crate::ops::open_url("ms-settings:defaultapps"); }
            });
            if self.tools.repair.inspection.as_ref().is_some_and(|s| s.user_choice.is_some()) {
                ui.label(hint("Windows has a protected default-app override. After registration, use Default apps to select the Unreal project handler; the tool does not override UserChoice."));
            }
            ui.label(hint("If the association check is OK and double-click works, leave the registration unchanged."));
            ui.label(hint("Selector missing? Use Epic Launcher to repair/reinstall its components. An engine Verify can repair engine files; it does not guarantee the Launcher selector is restored."));
        });
        ui.add_space(10.0);
        card().show(ui, |ui| {
            ui.label(heading("3. Preview targeted project setting repairs", 14.0));
            ui.label(hint("Close Unreal Editor before applying. Each change keeps a unique adjacent backup. Assets, plugins, source, autosaves and the game default map are preserved."));
            ui.add_enabled_ui(self.project_path.is_some() && !self.is_busy_now(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, fix) in [
                        ("Use selected engine", FileFix::EngineAssociation),
                        ("Clear editor startup map", FileFix::ClearStartupMap),
                        ("Set Windows renderer to DX11", FileFix::UseDx11),
                        ("Reset generated editor preferences", FileFix::ResetEditorPreferences),
                    ] {
                        if ui.button(label).clicked() {
                            match repair::plan_file(self.project_path.as_ref().unwrap(), self.engine_dir.as_deref(), fix) {
                                Ok(plan) => self.tools.repair.set_preview(Preview::File(plan)),
                                Err(e) => self.tools.repair.notice = e,
                            }
                        }
                    }
                });
            });
            ui.label(hint("Missing modules/plugins or invalid project JSON need their own fixes. This workflow does not disable all plugins or erase generated folders."));
        });
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if let Some(undo) = &self.tools.repair.undo
                && ui.button("Preview undo last settings repair").clicked() {
                    self.tools
                        .repair
                        .set_preview(Preview::RestoreFile(undo.clone()));
                }
            if let Some(path) = &self.tools.repair.association_backup
                && ui.button("Preview undo Windows repair").clicked() {
                    self.preview_association_restore(path.clone());
                }
            if ui.button("Restore Windows repair from backup...").clicked() {
                let mut dialog = rfd::FileDialog::new().add_filter("Association backup", &["json"]);
                if let Some(dir) = crate::config::config_dir() {
                    dialog = dialog.set_directory(dir.join("repairs"));
                }
                if let Some(path) = dialog.pick_file() {
                    self.preview_association_restore(path);
                }
            }
        });
        if !self.tools.repair.notice.is_empty() {
            ui.label(&self.tools.repair.notice);
        }
        self.show_repair_preview(ui);
    }

    fn show_repair_preview(&mut self, ui: &mut egui::Ui) {
        let Some(preview) = &self.tools.repair.preview else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let response = callout(AMBER).show(ui, |ui| {
            ui.label(heading("Review before continuing", 14.0));
            match preview {
                Preview::File(p) => {
                    ui.label(&p.summary);
                    ui.monospace(p.path.display().to_string());
                    egui::CollapsingHeader::new("Before / after (first 80 lines)").show(ui, |ui| {
                        ui.label("Before");
                        ui.monospace(String::from_utf8_lossy(&p.before).lines().take(80).collect::<Vec<_>>().join("\n"));
                        ui.label("After");
                        ui.monospace(String::from_utf8_lossy(&p.after).lines().take(80).collect::<Vec<_>>().join("\n"));
                    });
                }
                Preview::Association(p) => {
                    ui.label("Writes only the listed HKCU values. A JSON backup is saved under UnrealDevTool/repairs first. Restore will refuse if these values change later.");
                    for c in &p.changes { let value = match repair::registry_value_name(&c.path) { "" => "(Default)", v => v }; ui.monospace(format!("{} [value: {}]\n  {:?} -> {}", c.path, value, c.before, c.after)); }
                }
                Preview::RestoreFile(u) => { ui.label("Restore the pre-repair contents. Refuses if the file or backup changed since this repair."); ui.monospace(u.backup.display().to_string()); }
                Preview::RestoreAssociation(p, plan) => { ui.label("Restore the association values recorded in this backup. Unexpected paths and changed current values are refused. Empty registry keys may remain."); ui.monospace(p.display().to_string()); for c in &plan.changes { ui.monospace(format!("{} [value: {}]\n  {} -> {:?}", c.path, repair::registry_value_name(&c.path), c.after, c.before)); } }
                Preview::Launch(exe, args) => {
                    ui.label("Launches the selected editor with the project as a separate argument. No shell command or permanent renderer setting is used. The editor can write project files on startup; use a copy if the engine differs.");
                    ui.monospace(format!("{}\nArguments: {args:?}", exe.display()));
                }
            }
            ui.horizontal(|ui| {
                apply = ui.add_enabled(!self.is_busy_now(), egui::Button::new("Confirm action")).clicked();
                cancel = ui.button("Cancel preview").clicked();
            });
        });
        if self.tools.repair.focus_preview {
            ui.scroll_to_rect(response.response.rect, Some(egui::Align::Min));
            self.tools.repair.focus_preview = false;
        }
        if cancel {
            self.tools.repair.preview = None;
        }
        if !apply {
            return;
        }
        let Some(action) = self.tools.repair.preview.take() else {
            return;
        };
        // Recheck immediately before mutating project files, not just at scan time.
        if matches!(action, Preview::File(_) | Preview::RestoreFile(_))
            && crate::ops::package::is_editor_running()
        {
            self.tools.repair.notice = "Close Unreal Editor and commandlets before changing project settings. Nothing changed.".into();
            return;
        }
        let result = match action {
            Preview::File(p) => repair::apply_file(&p).map(|undo| {
                let message = format!("Repair applied. Backup: {}. Try opening the project, then scan again.", undo.backup.display());
                self.tools.repair.undo = Some(undo);
                message
            }),
            Preview::Association(p) => repair::apply_association(&p).map(|path| {
                let message = format!("Per-user registration applied. Backup: {}. Test double-click; Windows UserChoice may still require Default apps.", path.display());
                self.tools.repair.association_backup = Some(path);
                message
            }),
            Preview::RestoreFile(undo) => repair::restore_file(&undo).map(|_| { self.tools.repair.undo = None; "Original settings restored.".into() }),
            Preview::RestoreAssociation(p, plan) => repair::restore_association(&p, &plan).map(|_| { self.tools.repair.association_backup = None; "Previous association values restored.".into() }),
            Preview::Launch(exe, args) => crate::ops::tools::launch(&exe, &args).map_err(|e| e.to_string()).map(|_| "Editor process started. This does not confirm that project loading succeeded; inspect the editor and its Saved/Logs output.".into()),
        };
        self.tools.repair.notice =
            result.unwrap_or_else(|e| format!("Action refused or failed: {e}"));
        self.refresh_editor_repair();
    }
}
