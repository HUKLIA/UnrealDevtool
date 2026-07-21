use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::{GitAction, GitState};

impl DevToolApp {
    pub fn show_git_panel(&mut self, ui: &mut egui::Ui) -> GitAction {
        match self.git_state.clone() {
            GitState::Idle                => GitAction::None,
            GitState::Menu                => self.show_git_menu_panel(ui),
            GitState::CommitMsg           => self.show_git_commit_panel(ui),
            GitState::SyncConfirm         => self.show_git_sync_panel(ui),
            GitState::MergeConfirm        => self.show_git_merge_panel(ui),
            GitState::AfterPush           => self.show_git_after_push_panel(ui),
            GitState::AfterMerge          => self.show_git_after_merge_panel(ui),
            GitState::NewBranchAfterPush  => self.show_git_new_branch_panel(ui, false),
            GitState::NewBranchAfterMerge => self.show_git_new_branch_panel(ui, true),
        }
    }

    fn show_git_menu_panel(&mut self, ui: &mut egui::Ui) -> GitAction {
        let on_main      = self.git_current_branch == "main";
        let branch_label = format!("Branch: {}", self.git_current_branch);

        Self::git_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("🐙  Git").size(13.0).color(accent()));
            ui.add_space(2.0);
            ui.label(egui::RichText::new(&branch_label).size(11.0).color(egui::Color32::GRAY));
            ui.add_space(10.0);

            let w = [ui.available_width(), 36.0];
            if ui.add_sized(w, egui::Button::new("📤  Commit & Push  (current branch)")).clicked() {
                self.git_state = GitState::CommitMsg;
            }
            ui.add_space(5.0);
            if ui.add_sized(w, egui::Button::new("🔄  Sync  (fetch + rebase on main)")).clicked() {
                self.git_state = GitState::SyncConfirm;
            }
            ui.add_space(5.0);
            ui.add_enabled_ui(!on_main, |ui| {
                if ui.add_sized(w, egui::Button::new("🔀  Merge current branch  >>  main")).clicked() {
                    self.git_state = GitState::MergeConfirm;
                }
            });
            if on_main {
                ui.label(egui::RichText::new("  Already on main — switch to a feature branch first")
                    .size(10.0).color(HINT_GRAY));
            }
            ui.add_space(10.0);
            if ui.add_sized([ui.available_width(), 26.0], egui::Button::new("—  Cancel")).clicked() {
                self.git_state = GitState::Idle;
            }
        });
        GitAction::None
    }

    fn show_git_commit_panel(&mut self, ui: &mut egui::Ui) -> GitAction {
        let mut action = GitAction::None;
        let can_commit = !self.git_commit_msg.trim().is_empty();
        let branch     = self.git_current_branch.clone();

        Self::git_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("📤  Commit & Push").size(13.0).color(accent()));
            ui.label(egui::RichText::new(format!(">>  {}", branch)).size(11.0).color(egui::Color32::GRAY));
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Commit message:").size(11.0).color(egui::Color32::GRAY));
            let resp = ui.add(
                egui::TextEdit::multiline(&mut self.git_commit_msg)
                    .hint_text("What did you change?")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            if resp.has_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl)
                && can_commit
            {
                action = GitAction::StartCommitPush;
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Tip: Ctrl+Enter to submit").size(10.0).color(HINT_GRAY));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(can_commit, |ui| {
                    if ui.add_sized([190.0, 32.0], egui::Button::new(">>  Commit & Push")).clicked() {
                        action = GitAction::StartCommitPush;
                    }
                });
                if ui.add_sized([90.0, 32.0], egui::Button::new("« Back")).clicked() {
                    self.git_state = GitState::Menu;
                }
            });
        });
        action
    }

    fn show_git_sync_panel(&mut self, ui: &mut egui::Ui) -> GitAction {
        let mut action = GitAction::None;
        let branch     = self.git_current_branch.clone();

        Self::git_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("🔄  Sync").size(13.0).color(accent()));
            ui.label(egui::RichText::new(format!("Branch: {}", branch)).size(11.0).color(egui::Color32::GRAY));
            ui.add_space(8.0);
            Self::code_block().show(ui, |ui| {
                ui.label(egui::RichText::new("  1.  git fetch origin main").size(11.0).color(egui::Color32::LIGHT_GRAY));
                ui.label(egui::RichText::new("  2.  git rebase origin/main").size(11.0).color(egui::Color32::LIGHT_GRAY));
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(
                "If a conflict occurs you will be asked to open Fork to resolve it."
            ).size(10.0).color(WARN_AMBER));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add_sized([190.0, 32.0], egui::Button::new(">>  Fetch & Rebase")).clicked() {
                    action = GitAction::StartSync;
                }
                if ui.add_sized([90.0, 32.0], egui::Button::new("« Back")).clicked() {
                    self.git_state = GitState::Menu;
                }
            });
        });
        action
    }

    fn show_git_merge_panel(&mut self, ui: &mut egui::Ui) -> GitAction {
        let mut action  = GitAction::None;
        let from_branch = self.git_current_branch.clone();

        Self::git_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("🔀  Merge to Main").size(13.0).color(accent()));
            ui.label(egui::RichText::new(format!("{}  >>  main", from_branch)).size(11.0).color(egui::Color32::GRAY));
            ui.add_space(8.0);
            Self::code_block().show(ui, |ui| {
                ui.label(egui::RichText::new("  1.  git checkout main").size(11.0).color(egui::Color32::LIGHT_GRAY));
                ui.label(egui::RichText::new("  2.  git pull origin main").size(11.0).color(egui::Color32::LIGHT_GRAY));
                ui.label(egui::RichText::new(format!("  3.  git merge {}", from_branch)).size(11.0).color(egui::Color32::LIGHT_GRAY));
                ui.label(egui::RichText::new("  4.  git push origin main  (no force)").size(11.0).color(egui::Color32::LIGHT_GRAY));
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(
                "If a conflict occurs you will be asked to open Fork to resolve it."
            ).size(10.0).color(WARN_AMBER));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add_sized([140.0, 32.0], egui::Button::new(">>  Merge only")).clicked() {
                    action = GitAction::StartMerge;
                }
                if ui.add_sized([155.0, 32.0], egui::Button::new("📦  Merge + Package")).clicked() {
                    action = GitAction::StartMergeAndPackage;
                }
                if ui.add_sized([80.0, 32.0], egui::Button::new("« Back")).clicked() {
                    self.git_state = GitState::Menu;
                }
            });
        });
        action
    }

    fn show_git_after_push_panel(&mut self, ui: &mut egui::Ui) -> GitAction {
        let branch = self.git_current_branch.clone();

        Self::git_frame().show(ui, |ui| {
            ui.colored_label(accent(), format!("[OK]  Pushed to  {}", branch));
            ui.add_space(8.0);
            ui.label(egui::RichText::new("What next?").size(11.0).color(egui::Color32::GRAY));
            ui.add_space(6.0);
            let w = [ui.available_width(), 34.0];
            if ui.add_sized(w, egui::Button::new(format!("🔖  Stay on  {}", branch))).clicked() {
                self.git_state = GitState::Idle;
            }
            ui.add_space(5.0);
            if ui.add_sized(w, egui::Button::new(format!("🌿  New branch based on  {}", branch))).clicked() {
                self.git_new_branch_name.clear();
                self.git_state = GitState::NewBranchAfterPush;
            }
            ui.add_space(8.0);
            if ui.add_sized([ui.available_width(), 26.0], egui::Button::new("—  Done")).clicked() {
                self.git_state = GitState::Idle;
            }
        });
        GitAction::None
    }

    fn show_git_after_merge_panel(&mut self, ui: &mut egui::Ui) -> GitAction {
        let mut action  = GitAction::None;
        let merged_from = self.git_merged_from.clone();

        Self::git_frame().show(ui, |ui| {
            ui.colored_label(accent(), format!("[OK]  Merged {}  >>  main", merged_from));
            ui.add_space(8.0);
            ui.label(egui::RichText::new("What next?").size(11.0).color(egui::Color32::GRAY));
            ui.add_space(6.0);
            let w = [ui.available_width(), 34.0];
            if ui.add_sized(w, egui::Button::new(format!("🔙  Back to  {}", merged_from))).clicked() {
                action = GitAction::StartCheckout { branch: merged_from.clone() };
            }
            ui.add_space(5.0);
            if ui.add_sized(w, egui::Button::new("🌿  New branch based on main")).clicked() {
                self.git_new_branch_name.clear();
                self.git_state = GitState::NewBranchAfterMerge;
            }
            ui.add_space(8.0);
            if ui.add_sized([ui.available_width(), 26.0], egui::Button::new("—  Stay on main")).clicked() {
                self.git_state = GitState::Idle;
            }
        });
        action
    }

    fn show_git_new_branch_panel(&mut self, ui: &mut egui::Ui, after_merge: bool) -> GitAction {
        let mut action = GitAction::None;
        let can_create = !self.git_new_branch_name.trim().is_empty();
        let base_label = if after_merge {
            "main".to_string()
        } else {
            self.git_current_branch.clone()
        };

        Self::git_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("🌿  New Branch").size(13.0).color(accent()));
            ui.label(egui::RichText::new(format!("Based on: {}", base_label)).size(11.0).color(egui::Color32::GRAY));
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Branch name:").size(11.0).color(egui::Color32::GRAY));
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.git_new_branch_name)
                    .hint_text("feature/my-thing")
                    .desired_width(f32::INFINITY),
            );
            if resp.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && can_create
            {
                action = GitAction::StartNewBranch { name: self.git_new_branch_name.trim().to_string() };
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(can_create, |ui| {
                    if ui.add_sized([190.0, 32.0], egui::Button::new(">>  Create Branch")).clicked() {
                        action = GitAction::StartNewBranch { name: self.git_new_branch_name.trim().to_string() };
                    }
                });
                if ui.add_sized([90.0, 32.0], egui::Button::new("« Back")).clicked() {
                    self.git_state = if after_merge { GitState::AfterMerge } else { GitState::AfterPush };
                }
            });
        });
        action
    }

    /// Companion panel for the Git tab's right column — real repo state
    /// (uncommitted file count, last commit, ahead/behind vs. the local
    /// tracking ref, a 14-day commit-activity bar chart, and working-tree
    /// diffstat), not a decorative placeholder. Shown next to the action
    /// panel on every git sub-screen so the tab isn't a single narrow card
    /// adrift in empty space.
    ///
    /// Takes `&self`, not `&mut self`: this only ever reads `self.git_status`,
    /// which is refreshed at its two actual call sites — `open_git_menu` in
    /// app.rs and the git-task-finished handler in `ui::mod`'s `update` —
    /// not here. Shelling out to git 60 times a second just to paint a panel
    /// would be wasteful and would make the UI stutter on a slow disk/repo.
    pub fn show_git_status_panel(&self, ui: &mut egui::Ui) {
        Self::git_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("📊  Repo Status").size(13.0).color(accent()));
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(format!("Branch: {}", self.git_current_branch))
                    .size(11.0).color(egui::Color32::GRAY),
            );
            ui.add_space(10.0);

            let (label, color) = if self.git_status.uncommitted == 0 {
                ("[OK]  Working tree clean".to_string(), accent())
            } else {
                (format!("[!]  {} uncommitted change(s)", self.git_status.uncommitted), WARN_AMBER)
            };
            ui.colored_label(color, label);
            ui.add_space(8.0);

            ui.label(egui::RichText::new("LAST COMMIT").size(9.5).color(HINT_GRAY));
            ui.add_space(2.0);
            match &self.git_status.last_commit {
                Some(c) => { ui.label(egui::RichText::new(c).size(11.0).color(egui::Color32::LIGHT_GRAY)); }
                None    => { ui.label(egui::RichText::new("No commits yet").size(11.0).color(HINT_GRAY)); }
            }
            ui.add_space(8.0);

            ui.label(egui::RichText::new("VS. UPSTREAM  (as of last fetch)").size(9.5).color(HINT_GRAY));
            ui.add_space(2.0);
            match self.git_status.ahead_behind {
                Some((0, 0)) => { ui.colored_label(accent(), "Up to date with upstream"); }
                Some((ahead, behind)) => {
                    ui.label(
                        egui::RichText::new(format!("↑ {} ahead   ↓ {} behind", ahead, behind))
                            .size(11.0).color(egui::Color32::LIGHT_GRAY),
                    );
                }
                None => {
                    ui.label(
                        egui::RichText::new("No upstream tracking branch set")
                            .size(10.5).color(HINT_GRAY),
                    );
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(egui::RichText::new("COMMIT ACTIVITY  (LAST 14 DAYS)").size(9.5).color(HINT_GRAY));
            ui.add_space(4.0);
            if self.git_status.activity.is_empty() {
                // Empty means the underlying `git log` failed outright (no
                // repo, no commits at all, git missing) — not "14 real
                // zeros" — so this shows an explicit "no data" message
                // instead of a flat, misleading chart.
                ui.label(egui::RichText::new("No commit history").size(10.5).color(HINT_GRAY));
            } else {
                crate::ui::bar_chart::show_bar_chart(ui, &self.git_status.activity, 70.0);
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(egui::RichText::new("WORKING TREE").size(9.5).color(HINT_GRAY));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("{} file(s) changed", self.git_status.changed_files))
                    .size(11.0).color(egui::Color32::LIGHT_GRAY),
            );
            ui.horizontal(|ui| {
                ui.colored_label(accent(), format!("+{}", self.git_status.insertions));
                ui.add_space(8.0);
                ui.colored_label(ERR_RED, format!("-{}", self.git_status.deletions));
            });
        });
    }

    // ── Shared frame builders ─────────────────────────────────────────────────

    fn git_frame() -> egui::Frame {
        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(14.0))
    }

    fn code_block() -> egui::Frame {
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 18, 26))
            .rounding(egui::Rounding::same(4.0))
            .inner_margin(egui::Margin::same(6.0))
    }
}
