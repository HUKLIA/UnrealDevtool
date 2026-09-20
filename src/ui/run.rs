use eframe::egui;
use crate::app::DevToolApp;
use crate::ops::run::{Level, Stage};
use crate::theme::*;
use crate::types::{BuildConfiguration, BuildTarget, RunState};
use crate::ui::shell::clock;

impl DevToolApp {
    /// The run surface: one screen that follows the job through Ready →
    /// Running → Done. Which one is showing is derived, never chosen.
    /// The run surface: the build column, and the context rail beside or
    /// below it depending on how much room there is.
    ///
    /// Two rules keep this working at any window size:
    ///
    /// * Nothing is stretched to a computed height. Cards are their natural
    ///   size and a region scrolls when its content does not fit, which is
    ///   what a browser does and what stops borders being clipped or cards
    ///   overlapping when space runs short.
    /// * Exactly one scroll region is ever in play for a given piece of
    ///   content. Wide, the two columns scroll independently; narrow, one
    ///   outer scroll owns the whole stack and the columns do not scroll at
    ///   all. Nesting them put two scrollbars side by side and made the wheel
    ///   act on whichever region the pointer happened to be over.
    pub fn show_run_surface(&mut self, ui: &mut egui::Ui, state: RunState) {
        // Stacking is decided by whether both columns can be usable, not by a
        // magic number. The old flat 500pt threshold kept two columns between
        // 500 and ~630pt while squeezing the rail too narrow to read, so the
        // sizes that looked worst were the ones just above the cutoff.
        const RAIL_MIN: f32 = 250.0;
        const MAIN_MIN: f32 = 360.0;
        const COL_GAP:  f32 = 18.0;

        // `available_rect_before_wrap` gives both the origin and the size of
        // what is actually left. `max_rect().min` is the top of the panel, not
        // the cursor, so with the update notice above it the row started too
        // high and ran past the bottom edge — the columns were cut off with
        // dead space under them.
        let region = ui.available_rect_before_wrap();
        let avail  = region.size();
        if avail.x < RAIL_MIN + MAIN_MIN + COL_GAP {
            egui::ScrollArea::vertical()
                .id_salt("stacked_run_surface")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.show_main_column(ui, state, false);
                    ui.add_space(COL_GAP);
                    self.show_rail_column(ui, state, false);
                    ui.add_space(GUTTER);
                });
            return;
        }

        // Two columns, each given the full viewport height and scrolling on
        // its own. The row is split into explicit rectangles rather than laid
        // out with a horizontal cursor: the main column grows past the width
        // it is handed when its content needs more, which used to drift the
        // cursor and carry the rail past the panel's right margin.
        let row     = region;
        let rail_w  = (row.width() * 0.32).clamp(RAIL_MIN, 360.0);
        let main_w  = (row.width() - rail_w - COL_GAP).max(MAIN_MIN);
        let main_rect = egui::Rect::from_min_size(row.min, egui::vec2(main_w, row.height()));
        let rail_rect = egui::Rect::from_min_size(
            egui::pos2(row.max.x - rail_w, row.min.y),
            egui::vec2(rail_w, row.height()),
        );
        ui.allocate_rect(row, egui::Sense::hover());

        let td = egui::Layout::top_down(egui::Align::Min);
        let mut main_ui = ui.new_child(egui::UiBuilder::new().max_rect(main_rect).layout(td));
        self.show_main_column(&mut main_ui, state, true);

        let mut rail_ui = ui.new_child(egui::UiBuilder::new().max_rect(rail_rect).layout(td));
        self.show_rail_column(&mut rail_ui, state, true);
    }

    /// The build column. `scroll` is false when an outer region already owns
    /// scrolling for the whole stack.
    fn show_main_column(&mut self, ui: &mut egui::Ui, state: RunState, scroll: bool) {
        // Running draws its own full-height log pane, so it manages its space
        // itself and must not be wrapped in a second scroll region.
        if matches!(state, RunState::Running) {
            self.show_running(ui, scroll);
            return;
        }
        let body = |app: &mut Self, ui: &mut egui::Ui| match state {
            RunState::Done => app.show_done(ui),
            _              => app.show_ready(ui),
        };
        if scroll {
            egui::ScrollArea::vertical()
                .id_salt("main_column")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    body(self, ui);
                    // The panel has no bottom margin (see `update`), so the
                    // gap under the last card lives inside the scroll area:
                    // cards run off the bottom edge as they scroll instead of
                    // being cut off above an empty band.
                    ui.add_space(GUTTER);
                });
        } else {
            body(self, ui);
        }
    }

    fn show_rail_column(&mut self, ui: &mut egui::Ui, state: RunState, scroll: bool) {
        match state {
            RunState::Running => self.show_run_rail(ui, scroll),
            RunState::Done    => self.show_done_rail(ui, scroll),
            _                 => self.show_context_rail(ui, scroll),
        }
    }

    // ── Ready ───────────────────────────────────────────────────────────────

    fn show_ready(&mut self, ui: &mut egui::Ui) {
        let mut start_requested = false;
        let project = self.project_path.clone();

        let auto_version = crate::ops::package::format_version(self.next_version_preview);
        let version = if self.use_custom_version {
            self.version_override.trim().to_string()
        } else {
            auto_version.clone()
        };

        let name_err = crate::ops::package::validate_leaf_name(&self.pack_name_input, "Package name").err();
        let exe_err  = crate::ops::package::validate_leaf_name(&self.exe_name_input, "Executable name").err();
        let ver_err  = crate::ops::package::validate_leaf_name(&version, "Version").err();
        let can_start = name_err.is_none() && exe_err.is_none() && ver_err.is_none()
            && crate::ops::package::parse_extra_uat_args(&self.extra_uat_args).is_ok();

        let mut changed = false;

        let config_response = section().show(ui, |ui| {
            // Fill the column. A frame otherwise takes its content's width,
            // so any child a little too wide widened the whole card.
            ui.set_min_width(ui.available_width());
            // Headline: what is about to be built.
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(eyebrow("NEXT BUILD"));
                    ui.add_space(7.0);
                    let title = project.as_ref()
                        .and_then(|p| p.file_stem())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "—".into());
                    ui.label(heading(&title, 32.0));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.vertical(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.label(eyebrow("VERSION"));
                        });
                        ui.add_space(7.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.label(numeral(&version, 25.0, accent()));
                        });
                    });
                });
            });

            // Configuration, as chips rather than a form.
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                // Platform first: it changes what the rest of the row means,
                // and it is the thing most likely to be wrong when a build
                // fails for a reason the log does not explain.
                let platform_start = ui.cursor().min;
                for t in BuildTarget::ALL {
                    let on = self.build_target == t;
                    let usable = t.buildable_on_windows();
                    let resp = ui.add_enabled(usable, chip(t.label(), on));
                    let resp = if usable {
                        resp.on_hover_text(format!("Package for {} (-platform={})", t.label(), t.uat_name()))
                    } else {
                        resp.on_hover_text("Mac packaging has to run on macOS — Unreal cannot cross-compile it from Windows.")
                    };
                    if resp.clicked() && !on && usable {
                        self.build_target = t;
                        self.refresh_doctor();
                        changed = true;
                    }
                }
                // The whole row of platform chips is the thing the manual
                // points at, not any single one of them.
                let platform_rect = egui::Rect::from_min_max(
                    platform_start,
                    egui::pos2(ui.cursor().min.x, platform_start.y + 28.0),
                );
                self.guide_anchor(crate::ui::guide::step::PLATFORM, platform_rect);
                ui.add_space(10.0);

                for cfg in [BuildConfiguration::Shipping, BuildConfiguration::Development] {
                    let on = self.build_configuration == cfg;
                    if ui.add(chip(cfg.as_str(), on)).clicked() && !on {
                        self.build_configuration = cfg;
                        changed = true;
                    }
                }
                // Iterative cook. The biggest time saver UAT offers and the
                // one the tool never exposed: on a project that has been built
                // once, this turns the cook stage from minutes into seconds.
                let iter_on = self.iterate_cook;
                if ui.add(chip("Iterate", iter_on))
                    .on_hover_text(if iter_on {
                        "Reusing the previous cook — only changed assets are processed.\n\
                         Turn off for a release build: a stale cook can hide a content problem."
                    } else {
                        "Full cook from scratch. Turn on to reuse the previous cook and \
                         process only what changed — much faster for repeat builds."
                    })
                    .clicked() {
                    self.iterate_cook = !iter_on;
                }
                let auto_on = !self.use_custom_version;
                if ui.add(chip(if auto_on { "Auto-version" } else { "Custom version" }, auto_on)).clicked() {
                    self.use_custom_version = !self.use_custom_version;
                    if self.use_custom_version && self.version_override.trim().is_empty() {
                        self.version_override = auto_version.clone();
                    }
                }
            });

            // How it is packaged. Three real alternatives, not three labels for
            // one thing — see `types::PackageMethod`.
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(hint("Method"));
                for m in crate::types::PackageMethod::ALL {
                    let on = self.package_method == m;
                    if ui.add(chip(m.label(), on)).on_hover_text(m.hint()).clicked() && !on {
                        self.package_method = m;
                        if let Some(p) = &self.project_path {
                            crate::config::save_uat_options(p, self.compress_pak, &self.extra_uat_args, m);
                        }
                    }
                }
            });

            // Keep names editable in both modes, with the optional custom
            // version field sharing the same wrapped row.
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if self.use_custom_version {
                    ui.label(hint("Version"));
                    changed |= ui.add_sized([120.0, 26.0],
                        egui::TextEdit::singleline(&mut self.version_override)).changed();
                }
                // Sized from what is left rather than two fixed 150px boxes,
                // which wrapped the Exe field onto its own line and left the
                // label stranded above it.
                let field_w = ((ui.available_width() - 108.0) / 2.0).clamp(96.0, 190.0);
                ui.label(hint("Package"));
                changed |= ui.add_sized([field_w, 26.0],
                    egui::TextEdit::singleline(&mut self.pack_name_input)).changed();
                ui.add_space(6.0);
                ui.label(hint("Exe"));
                changed |= ui.add_sized([field_w, 26.0],
                    egui::TextEdit::singleline(&mut self.exe_name_input)).changed();
            });
            // Output path on its own line.
            //
            // It rode along at the end of the chip row, right-aligned. Inside
            // `horizontal_wrapped` the available width resets per wrapped
            // line, so the guard meant to hide it never fired and it collided
            // with the last chip instead.
            ui.add_space(6.0);
            ui.add(egui::Label::new(
                numeral(&format!("-> build/{version}/"), 11.0, DIM)).truncate());

            // Options most builds never touch, kept out of the way.
            ui.add_space(6.0);
            let extra_err = crate::ops::package::parse_extra_uat_args(&self.extra_uat_args).err();
            egui::CollapsingHeader::new(egui::RichText::new("Advanced").font(body(12.0)).color(MUTED))
                .id_salt("advanced_options")
                .default_open(!self.extra_uat_args.trim().is_empty() || self.compress_pak || self.schedule.is_some() || self.size_budget_mb > 0)
                .show(ui, |ui| {
                    let mut adv_changed = false;
                    ui.horizontal_wrapped(|ui| {
                        let on = self.compress_pak;
                        if ui.add(chip("Compress pak", on))
                            .on_hover_text("Pass -compressed: smaller packages, slower to build.")
                            .clicked() {
                            self.compress_pak = !on;
                            adv_changed = true;
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(hint("Extra UAT arguments"));
                    let w = ui.available_width();
                    adv_changed |= ui.add_sized([w, 26.0],
                        egui::TextEdit::singleline(&mut self.extra_uat_args)
                            .hint_text("-nocompile -ddc=Shared")).changed();
                    if let Some(e) = &extra_err {
                        ui.label(egui::RichText::new(e).font(body(11.5)).color(RED));
                    }

                    // A size limit worth warning about: a store cap, or a
                    // number the team agreed. The result screen flags a build
                    // that goes over.
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(hint("Size budget (MB)"));
                        if ui.add_sized([80.0, 26.0],
                            egui::TextEdit::singleline(&mut self.size_budget_input).hint_text("none")).changed() {
                            self.size_budget_mb = self.size_budget_input.trim().parse().unwrap_or(0);
                            if let Some(p) = &self.project_path {
                                crate::config::save_size_budget(p, self.size_budget_mb);
                            }
                        }
                    });

                    // Start unattended at a time of day — the usual answer to
                    // a 30-minute build that should not compete with the
                    // working day.
                    ui.add_space(8.0);
                    ui.label(hint("Start at"));
                    ui.horizontal(|ui| {
                        match self.schedule.clone() {
                            Some((at, label)) => {
                                let left = at.saturating_duration_since(std::time::Instant::now()).as_secs();
                                ui.label(egui::RichText::new(
                                    format!("Starts at {label} — in {}", crate::ops::clock::countdown(left)))
                                    .font(body(12.0)).color(accent()));
                                if ui.add(quiet("Cancel")).clicked() { self.schedule = None; }
                            }
                            None => {
                                ui.add_sized([70.0, 26.0],
                                    egui::TextEdit::singleline(&mut self.schedule_input).hint_text("23:30"));
                                let parsed = crate::ops::clock::parse_hhmm(&self.schedule_input);
                                if ui.add_enabled(parsed.is_some() && can_start, quiet("Schedule"))
                                    .on_hover_text("Starts the build at that time, if the app is still open on this screen. The Unreal Editor is closed when it starts.")
                                    .clicked()
                                    && let (Some((h, m)), Some(now)) = (parsed, crate::ops::clock::local_seconds_of_day()) {
                                    let wait = crate::ops::clock::secs_until(h, m, now);
                                    self.schedule = Some((
                                        std::time::Instant::now() + std::time::Duration::from_secs(wait),
                                        format!("{h:02}:{m:02}"),
                                    ));
                                }
                            }
                        }
                    });
                    if adv_changed && let Some(p) = &self.project_path {
                        crate::config::save_uat_options(p, self.compress_pak, &self.extra_uat_args, self.package_method);
                    }
                });

            for e in [&name_err, &exe_err, &ver_err].into_iter().flatten() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(e).font(body(11.5)).color(RED));
            }

            ui.add_space(20.0);
            divider(ui);
            ui.add_space(18.0);

            // The pipeline, idle.
            stage_columns(ui, 34.0, |ui, i, cell| {
                segment(ui, cell, 0.0, accent());
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    ui.label(numeral(&format!("{:02}", i + 1), 10.0, FAINT));
                    ui.add(egui::Label::new(
                        egui::RichText::new(Stage::ALL[i].label()).font(body(12.0)).color(MUTED)).truncate());
                });
            });

            // The action.
            ui.add_space(24.0);
            // Split from the width that is left. Two fixed sizes plus the
            // row's own item spacing came to more than a narrow card holds,
            // which pushed the card past its column.
            ui.horizontal(|ui| {
                let gap = ui.spacing().item_spacing.x;
                let total = ui.available_width();
                let start_w = ((total - gap) * 0.55).min(200.0);
                let vs_w = (total - gap - start_w).max(0.0);
                let start_response = ui.add_enabled_ui(can_start, |ui| {
                    ui.add_sized([start_w, 50.0], primary("Start build"))
                });
                self.guide_anchor(crate::ui::guide::step::START, start_response.inner.rect);
                if start_response.inner.clicked() {
                    start_requested = true;
                }
                if ui.add_sized([vs_w.min(170.0), 50.0], ghost("Rebuild VS files")).clicked() {
                    self.open_vs_config();
                }
            });
            ui.add_space(4.0);
            ui.label(if self.editor_is_running {
                egui::RichText::new("Unreal Editor is open — save first; it will close automatically.").font(body(11.0)).color(AMBER)
            } else {
                egui::RichText::new("Editor closed — ready to package.").font(body(11.0)).color(SOFT)
            });
        });
        self.guide_anchor(crate::ui::guide::step::CONFIGURE, config_response.response.rect);

        if changed && let Some(p) = self.project_path.clone() {
            crate::config::save_project_config(
                &p, self.pack_name_input.trim(), self.exe_name_input.trim(),
                self.build_configuration, self.build_target);
        }

        ui.add_space(14.0);
        self.show_prechecks(ui);

        if start_requested {
            self.start_packaging();
        }
    }

    /// Prechecks as a single line.
    ///
    /// This was a card of stacked rows, each with a label, a value and a
    /// coloured rule — a permanent quarter-screen of information that is
    /// almost always four greens. One line says the same thing, and the detail
    /// is one click away in the Diagnostics sheet.
    fn show_prechecks(&mut self, ui: &mut egui::Ui) {
        let mut open_details = false;
        card()
            .inner_margin(egui::Margin::symmetric(18.0, 13.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                // The button is laid out first, from the right, so the items
                // flow into what is left. Done the other way round the
                // right-to-left group overlapped the last check.
                // Width is split explicitly rather than letting two nested
                // layouts negotiate it. Both earlier attempts had the
                // right-aligned button drawn over the last check, because a
                // nested layout still measures against the full row.
                ui.horizontal(|ui| {
                    const BTN_W: f32 = 80.0;
                    let total = ui.available_width();
                    let items_w = (total - BTN_W - 10.0).max(120.0);

                    ui.allocate_ui_with_layout(
                        egui::vec2(items_w, 26.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(items_w);
                            ui.label(eyebrow("PRECHECKS"));
                            ui.add_space(8.0);
                            for item in self.precheck_summary() {
                                if ui.available_width() < 70.0 { break; }
                                dot(ui, item.1, 7.0);
                                ui.add_space(1.0);
                                ui.label(egui::RichText::new(item.0).font(body(12.0)).color(SOFT));
                                ui.add_space(3.0);
                                ui.add(egui::Label::new(
                                    egui::RichText::new(item.2).font(mono(11.5)).color(DIM)
                                ).truncate());
                                ui.add_space(12.0);
                            }
                        },
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let details = ui.add_sized([BTN_W, 26.0], quiet("Details"));
                        self.guide_anchor(crate::ui::guide::step::PRECHECKS, details.rect);
                        if details.clicked() {
                            open_details = true;
                        }
                    });
                });
            });
        if open_details {
            self.open_sheet(crate::types::Sheet::Diagnostics);
        }
    }

    /// Condenses the preflight items into (label, colour, value) triples.
    fn precheck_summary(&self) -> Vec<(&'static str, egui::Color32, String)> {
        use crate::ops::preflight::CheckStatus;
        let mut out = Vec::new();

        let engine_ok = self.engine_dir.is_some();
        out.push((
            "Engine",
            if engine_ok { GREEN } else { RED },
            self.engine_dir.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "missing".into()),
        ));

        out.push((
            "Project",
            if self.project_path.is_some() { GREEN } else { RED },
            if self.project_path.is_some() { ".uproject".into() } else { "unset".into() },
        ));

        let disk = self.pc_check_disk.lock().unwrap_or_else(|e| e.into_inner()).as_ref().cloned();
        let (dc, dv) = match &disk {
            Some(item) => (
                match item.status {
                    CheckStatus::Ok => GREEN,
                    CheckStatus::Warn => AMBER,
                    CheckStatus::Fail => RED,
                },
                item.detail.clone(),
            ),
            None => (MUTED, "checking…".into()),
        };
        out.push(("Disk", dc, dv));

        // Config problems found by the packaging-readiness check.
        {
            use crate::ops::preflight::CheckStatus;
            let fails = self.doctor_items.iter().filter(|i| matches!(i.status, CheckStatus::Fail)).count();
            let warns = self.doctor_items.iter().filter(|i| matches!(i.status, CheckStatus::Warn)).count();
            if fails + warns > 0 {
                let n = fails + warns;
                out.push(("Config", if fails > 0 { RED } else { AMBER },
                    format!("{n} {}", if n == 1 { "issue" } else { "issues" })));
            }
        }

        // The toolchain for a non-Windows target, right where the build is
        // started: it is the thing most likely to make a first Android or
        // Linux build fail, and it is cheap to know in advance.
        if let Some((ok, label, detail)) = crate::ops::preflight::platform_sdk_check(self.build_target) {
            // First, so a narrow strip drops the other checks before this one.
            out.insert(0, (label, if ok { GREEN } else { AMBER }, detail));
        }

        out.push((
            "Editor",
            if self.editor_is_running { AMBER } else { GREEN },
            if self.editor_is_running { "running".into() } else { "closed".into() },
        ));

        out
    }

    // ── Running ─────────────────────────────────────────────────────────────

    fn show_running(&mut self, ui: &mut egui::Ui, columns: bool) {
        let run = self.run.lock().unwrap_or_else(|e| e.into_inner());
        let current = run.current;
        let overall = run.overall();
        let fracs: Vec<f32> = Stage::ALL.iter().map(|s| run.stage_fraction(*s)).collect();
        let lines: Vec<_> = run.lines.iter().cloned().collect();
        drop(run);

        section()
            .stroke(egui::Stroke::new(1.0, acc(56)))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        let label = match current {
                            Some(Stage::Compile) => "COMPILING CODE",
                            Some(Stage::Cook)    => "COOKING CONTENT",
                            Some(Stage::Staging) => "STAGING FILES",
                            Some(Stage::Package) => "PACKAGING",
                            None                 => "STARTING",
                        };
                        ui.label(egui::RichText::new(label).font(body(10.5)).color(accent()));
                        ui.add_space(7.0);
                        let step = current.map(|s| s.index() + 1).unwrap_or(1);
                        ui.label(heading(&format!("Step {step} of 4"), 28.0));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        ui.vertical(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                ui.label(numeral(&format!("{}%", (overall * 100.0) as i32), 40.0, accent()));
                            });
                            if let Some(t0) = self.task_started_at {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                    ui.label(hint(&format!("elapsed {}", clock(t0.elapsed().as_secs()))));
                                });
                            }
                        });
                    });
                });

                ui.add_space(20.0);
                stage_columns(ui, 34.0, |ui, i, cell| {
                    let done = fracs[i] >= 1.0;
                    let active = current == Some(Stage::ALL[i]);
                    segment(ui, cell, fracs[i], if done { GREEN } else { accent() });
                    ui.add_space(7.0);
                    ui.horizontal(|ui| {
                        ui.label(numeral(&format!("{:02}", i + 1), 10.0, FAINT));
                        let c = if done { GREEN } else if active { TEXT } else { FAINT };
                        ui.add(egui::Label::new(
                            egui::RichText::new(Stage::ALL[i].label()).font(body(12.0)).color(c)).truncate());
                    });
                });
            });

        ui.add_space(14.0);

        // Live output. The log was previously a four-row box at the bottom of
        // the window showing our own status strings; this is UAT's own output,
        // which is what you actually want during a 30-minute build.
        // Total height of the pane, gutter included. Frame margins are the
        // vertical 12 + 12 of `deep()`.
        //
        // Side by side, the pane fills the column. Stacked, an outer scroll
        // area owns the page and "the rest of the height" is unbounded, so the
        // pane gets a fixed, readable height instead — filling it made the
        // page many screens tall.
        let h = if columns { (ui.available_height() - GUTTER).max(150.0) } else { 360.0 };
        deep().show(ui, |ui| {
            let inner = h - 24.0 - 2.0;
            ui.set_min_height(inner);
            let y0 = ui.cursor().min.y;
            ui.horizontal(|ui| {
                ui.label(eyebrow("OUTPUT"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    dot(ui, accent(), 7.0);
                    ui.label(hint("following"));
                });
            });
            ui.add_space(8.0);
            // Bounded to what is left in the pane. An unbounded scroll area
            // fills the whole region and pushes the pane's border into the
            // bottom gutter.
            let log_h = (inner - (ui.cursor().min.y - y0)).max(40.0);
            egui::ScrollArea::vertical()
                .id_salt("uat_log")
                .max_height(log_h)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if lines.is_empty() {
                        ui.label(hint("Waiting for UAT…"));
                    }
                    for l in &lines {
                        let c = match l.level {
                            Level::Error  => RED,
                            Level::Warn   => AMBER,
                            Level::Normal => egui::Color32::from_rgb(124, 152, 166),
                        };
                        ui.label(egui::RichText::new(&l.text).font(mono(11.5)).color(c));
                    }
                });
        });

        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }

    // ── Done ────────────────────────────────────────────────────────────────

    fn show_done(&mut self, ui: &mut egui::Ui) {
        let Some(out) = self.last_build.clone() else { return };
        let mut open_folder = false;
        let mut upload = false;
        let mut again = false;
        let mut next_target: Option<crate::types::BuildTarget> = None;

        let ok_color = if out.ok { GREEN } else { RED };
        section()
            .stroke(egui::Stroke::new(1.0, tint(ok_color, 62)))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let (mark, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                    ui.painter().rect_filled(mark, egui::Rounding::same(11.0), tint(ok_color, 36));
                    paint_icon(ui.painter(), mark.center(), if out.ok { Icon::Check } else { Icon::Cross }, ok_color);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(if out.ok { "BUILD COMPLETE" } else { "BUILD FAILED" })
                            .font(body(10.5)).color(ok_color));
                        ui.add_space(6.0);
                        ui.label(heading(&format!("{} {}", self.project_name(), out.version), 26.0));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        ui.vertical(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                ui.label(numeral(&clock(out.duration.as_secs()), 24.0, TEXT));
                            });
                            ui.add_space(3.0);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                ui.label(hint(&format!("{} · {}", out.config.as_str(), out.platform.label())));
                            });
                        });
                    });
                });

                // Result facts as tiles.
                ui.add_space(20.0);
                let tiles: [(&str, String); 3] = [
                    ("SIZE",     crate::ops::history::format_bytes(out.bytes)),
                    ("WARNINGS", out.warnings.to_string()),
                    ("ERRORS",   out.errors.to_string()),
                ];
                ui.columns(3, |cols| {
                    for (i, (k, v)) in tiles.iter().enumerate() {
                        well().show(&mut cols[i], |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(eyebrow(k));
                            ui.add_space(6.0);
                            let over_budget = *k == "SIZE" && self.size_budget_mb > 0
                                && out.bytes > self.size_budget_mb as u64 * 1024 * 1024;
                            let c = if over_budget { AMBER }
                                    else if *k == "ERRORS" && out.errors > 0 { RED }
                                    else if *k == "WARNINGS" && out.warnings > 0 { AMBER }
                                    else { TEXT };
                            ui.label(numeral(v, 16.0, c));
                            if over_budget {
                                let over = out.bytes - self.size_budget_mb as u64 * 1024 * 1024;
                                ui.label(egui::RichText::new(format!("{} over budget", crate::ops::history::format_bytes(over)))
                                    .font(body(11.0)).color(AMBER));
                            }
                        });
                    }
                });

                // The artifact itself.
                if let Some(zip) = &out.zip {
                    ui.add_space(14.0);
                    deep()
                        .inner_margin(egui::Margin::symmetric(14.0, 11.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (m, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                                paint_icon(ui.painter(), m.center(), Icon::Folder, DIM);
                                ui.add_space(3.0);
                                let text = zip.display().to_string();
                                ui.add(egui::Label::new(
                                    egui::RichText::new(&text).font(mono(11.5)).color(egui::Color32::from_rgb(143, 174, 188)),
                                ).truncate()).on_hover_text(&text);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add_sized([56.0, 24.0], quiet("Copy")).clicked() {
                                        ui.ctx().copy_text(text.clone());
                                    }
                                });
                            });
                        });
                }
            });

        // A failed build says why, on the surface, instead of leaving you to
        // find and read a multi-megabyte log.
        if !out.ok {
            ui.add_space(14.0);
            self.show_failure_summary(ui, &out);
        }

        // Log and report, for either outcome: the log is where a warning you
        // care about lives, and the report is what you paste into a bug.
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let has_log = out.log.is_some();
            if ui.add_enabled(has_log, quiet("Open build log")).clicked()
                && let Some(l) = &out.log {
                let _ = crate::ops::cmd("explorer").arg(l).spawn();
            }
            // Try the build. Only a Windows build can be started from here.
            if out.ok && out.platform == crate::types::BuildTarget::Win64 {
                let exe = out.zip.as_ref()
                    .and_then(|z| z.parent())
                    .map(|v| v.join(self.pack_name_input.trim()))
                    .and_then(|dir| crate::ops::package::find_main_exe(&dir).map(|e| (dir, e)));
                if ui.add_enabled(exe.is_some(), quiet("Run this build"))
                    .on_hover_text("Start the packaged game")
                    .clicked()
                    && let Some((dir, exe)) = exe {
                    match std::process::Command::new(&exe).current_dir(&dir).spawn() {
                        Ok(_)  => self.set_status(format!("Started {}.", exe.display())),
                        Err(e) => self.set_status(format!("[ERROR] Could not start the build: {e}")),
                    }
                }
            }
            if out.ok && self.git_project_dir().is_some()
                && ui.add(quiet("Copy release notes"))
                    .on_hover_text("The commits made since the previous build, ready to paste").clicked() {
                let notes = self.release_notes(&out);
                ui.ctx().copy_text(notes);
                self.set_status("Release notes copied to the clipboard.".into());
            }
            if ui.add(quiet("Copy report")).clicked() {
                let report = self.build_report(&out);
                ui.ctx().copy_text(report);
                self.set_status("Build report copied to the clipboard.".into());
            }
        });

        if out.ok && out.platform == crate::types::BuildTarget::Android {
            ui.add_space(14.0);
            self.show_android_install(ui, &out);
        }

        if !out.ok {
            // Nothing to ship. Offering "upload" for a build that failed is how
            // a half-built folder ends up on someone's drive.
            ui.add_space(14.0);
            section()
                .inner_margin(egui::Margin::symmetric(24.0, 20.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(eyebrow("NEXT"));
                    ui.add_space(13.0);
                    ui.columns(2, |cols| {
                        if ship_button(&mut cols[0], Icon::Play, "Try again", "Back to the build settings", true) {
                            again = true;
                        }
                        if ship_button(&mut cols[1], Icon::Help, "Project checks", "Preflight and clean-up", false) {
                            self.open_sheet(crate::types::Sheet::Diagnostics);
                        }
                    });
                });
            if again {
                self.dismiss_build();
                self.refresh_package_observed();
            }
            return;
        }

        // Ship actions — three equal choices, because which one is right
        // depends entirely on who the build is for.
        ui.add_space(14.0);
        section()
            .inner_margin(egui::Margin::symmetric(24.0, 20.0))
            .show(ui, |ui| {
                ui.label(eyebrow("SHIP IT"));
                ui.add_space(13.0);
                ui.columns(3, |cols| {
                    if ship_button(&mut cols[0], Icon::Folder, "Open folder", "Show the zip in Explorer", true) {
                        open_folder = true;
                    }
                    let drive_ready = crate::ops::rclone::is_available();
                    if ship_button(&mut cols[1], Icon::Copy, "Upload to Drive",
                        if drive_ready { "Send with rclone" } else { "rclone not installed" }, false) {
                        upload = true;
                    }
                    if ship_button(&mut cols[2], Icon::Play, "Build again", "Start the next version", false) {
                        again = true;
                    }
                });

                // The same project for another platform, one click from here.
                // It opens the Ready surface on that platform rather than
                // starting straight away: the version number for the next
                // build is worked out from disk and may not have caught up.
                let others: Vec<crate::types::BuildTarget> = crate::types::BuildTarget::ALL.into_iter()
                    .filter(|t| t.buildable_on_windows() && *t != out.platform)
                    .collect();
                if !others.is_empty() {
                    ui.add_space(14.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(hint("Then set up a build for"));
                        for t in others {
                            let sdk = crate::ops::preflight::platform_sdk_check(t);
                            let ready = sdk.as_ref().is_none_or(|(ok, _, _)| *ok);
                            let mut resp = ui.add_enabled(ready, chip(t.label(), false));
                            if let Some((false, label, detail)) = &sdk {
                                resp = resp.on_disabled_hover_text(format!("{label}: {detail}"));
                            }
                            if resp.clicked() { next_target = Some(t); }
                        }
                    });
                }
            });

        if open_folder && let Some(dir) = out.zip.as_ref().and_then(|z| z.parent()) {
            let _ = crate::ops::cmd("explorer").arg(dir).spawn();
        }
        if upload {
            if let Some(zip) = out.zip.clone() {
                self.upload_zip_path = zip;
            }
            self.gdrive_remote_status = None;
            self.show_upload_panel = true;
        }
        if let Some(t) = next_target {
            self.build_target = t;
            self.refresh_doctor();
            self.dismiss_build();
            self.refresh_package_observed();
            self.set_status(format!("Ready to build for {} — press Start build.", t.label()));
        }
        if again {
            self.dismiss_build();
            self.refresh_package_observed();
        }
    }

    /// Install the built APK on a connected phone, with adb.
    fn show_android_install(&mut self, ui: &mut egui::Ui, out: &crate::types::BuildOutcome) {
        use crate::ops::adb;
        // Results from the background threads.
        if let Some(r) = self.tools.adb_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.tools.adb_busy = false;
            match r {
                Ok(list) => self.tools.adb_devices = Some(list),
                Err(e)   => { self.tools.adb_devices = Some(Vec::new()); self.tools.adb_note = Some(e); }
            }
        }
        if let Some(msg) = self.tools.adb_result.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.tools.adb_busy = false;
            self.tools.adb_note = Some(msg);
        }

        // Looked up once for this build: finding the APK walks its output.
        let build_dir = out.zip.as_ref().and_then(|z| z.parent()).map(|p| p.to_path_buf()).unwrap_or_default();
        if self.tools.adb_lookup.as_ref().is_none_or(|(d, _, _)| *d != build_dir) {
            self.tools.adb_lookup = Some((build_dir.clone(), adb::find_adb(), adb::find_apk(&build_dir)));
            self.tools.adb_devices = None;
            self.tools.adb_note = None;
        }
        let (adb_path, apk) = self.tools.adb_lookup.as_ref().map(|(_, a, k)| (a.clone(), k.clone())).unwrap_or((None, None));
        let ctx = ui.ctx().clone();

        // Look for devices once when this screen first appears.
        if let Some(path) = &adb_path && self.tools.adb_devices.is_none() && !self.tools.adb_busy {
            self.tools.adb_busy = true;
            let (path, out, ctx) = (path.clone(), self.tools.adb_pending.clone(), ctx.clone());
            std::thread::spawn(move || {
                *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(adb::devices(&path));
                ctx.request_repaint();
            });
        }

        let mut refresh = false;
        let mut install: Option<String> = None;
        section().inner_margin(egui::Margin::symmetric(24.0, 20.0)).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(eyebrow("INSTALL ON A PHONE"));
            ui.add_space(10.0);
            match (&adb_path, &apk) {
                (None, _) => { ui.label(hint("adb was not found. It comes with the Android SDK's platform-tools — the SetupAndroid step installs it.")); }
                (_, None) => { ui.label(hint("No .apk was found in this build's output.")); }
                (Some(_), Some(apk)) => {
                    ui.label(hint(&format!("{}  ({})", apk.file_name().unwrap_or_default().to_string_lossy(),
                        crate::ops::history::format_bytes(std::fs::metadata(apk).map(|m| m.len()).unwrap_or(0)))));
                    ui.add_space(8.0);
                    let devices = self.tools.adb_devices.clone().unwrap_or_default();
                    ui.horizontal_wrapped(|ui| {
                        for d in &devices {
                            let label = format!("Install on {}", d.label());
                            let resp = ui.add_enabled(d.ready() && !self.tools.adb_busy, chip(&label, false));
                            if !d.ready() {
                                resp.on_disabled_hover_text(format!("{} — {}", d.serial,
                                    if d.state == "unauthorized" { "accept the USB debugging prompt on the phone" } else { &d.state }));
                            } else if resp.clicked() {
                                install = Some(d.serial.clone());
                            }
                        }
                        if ui.add_enabled(!self.tools.adb_busy, quiet("Refresh devices")).clicked() { refresh = true; }
                    });
                    if devices.is_empty() && !self.tools.adb_busy {
                        ui.add_space(4.0);
                        ui.label(hint("No device connected. Plug in a phone with USB debugging turned on."));
                    }
                    if self.tools.adb_busy {
                        ui.add_space(4.0);
                        ui.label(hint("Working…"));
                    }
                }
            }
            if let Some(n) = &self.tools.adb_note {
                ui.add_space(6.0);
                let bad = !n.starts_with("Installed");
                ui.label(egui::RichText::new(n).font(body(12.0)).color(if bad { RED } else { GREEN }));
            }
        });

        if refresh { self.tools.adb_devices = None; self.tools.adb_note = None; }
        if let (Some(serial), Some(path), Some(apk)) = (install, adb_path, apk) {
            self.tools.adb_busy = true;
            self.tools.adb_note = None;
            let res = self.tools.adb_result.clone();
            std::thread::spawn(move || {
                let msg = match adb::install(&path, &serial, &apk) { Ok(m) => m, Err(e) => e };
                *res.lock().unwrap_or_else(|e| e.into_inner()) = Some(msg);
                ctx.request_repaint();
            });
        }
        if self.tools.adb_busy { ui.ctx().request_repaint_after(std::time::Duration::from_millis(400)); }
    }

    /// Why a build failed: the recognised cause with its fix when the log
    /// matches a known signature, and the first error lines UAT printed
    /// either way.
    fn show_failure_summary(&self, ui: &mut egui::Ui, out: &crate::types::BuildOutcome) {
        section()
            .stroke(egui::Stroke::new(1.0, tint(RED, 62)))
            .inner_margin(egui::Margin::symmetric(24.0, 20.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(eyebrow("WHAT WENT WRONG"));
                ui.add_space(12.0);

                for d in &self.build_log_diagnosis {
                    ui.label(egui::RichText::new(&d.explanation).font(body(12.5)).color(TEXT));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("Fix: {}", d.fix)).font(body(12.0)).color(accent()));
                    ui.add_space(10.0);
                }
                if self.build_log_diagnosis.is_empty() && out.error_samples.is_empty() {
                    ui.label(hint("No error line was captured. The build log has the full output."));
                }
                if !out.error_samples.is_empty() {
                    ui.label(eyebrow("FIRST ERRORS"));
                    ui.add_space(6.0);
                    deep().inner_margin(egui::Margin::symmetric(12.0, 10.0)).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        for e in &out.error_samples {
                            ui.add(egui::Label::new(
                                egui::RichText::new(e).font(mono(11.0)).color(RED)).wrap());
                        }
                    });
                }
            });
    }

    /// Release notes: the commits since the previous build that recorded one.
    ///
    /// The commit each build was made from is saved in its `build-info.txt`,
    /// so "what changed" is just the git range between two builds — no notes
    /// to write by hand. With no earlier record it lists the latest commits.
    pub(crate) fn release_notes(&self, out: &crate::types::BuildOutcome) -> String {
        let Some(dir) = self.git_project_dir() else { return String::new() };
        let previous = self.builds.iter()
            .filter(|b| b.version != out.version)
            .find(|b| b.info.as_ref().is_some_and(|i| !i.commit.is_empty()));
        let since = previous.and_then(|b| b.info.as_ref()).map(|i| i.commit.as_str());
        let changes = crate::ops::git::changes_since(&dir, since, 15);

        let mut n = format!("{} {} — {} {}
", self.project_name(), out.version,
            out.platform.label(), out.config.as_str());
        match (previous, changes.is_empty()) {
            (_, true) => n.push_str("
No new commits since the previous build.
"),
            (Some(p), false) => n.push_str(&format!("
Changes since {} ({}):
", p.version, changes.len())),
            (None, false) => n.push_str(&format!("
Latest {} commits:
", changes.len())),
        }
        for c in &changes {
            n.push_str(&format!("- {c}
"));
        }
        n
    }

    /// A plain-text report of a finished build, for a bug report or a message.
    pub(crate) fn build_report(&self, out: &crate::types::BuildOutcome) -> String {
        let mut r = String::new();
        r.push_str(&format!("{} {} — {}
", self.project_name(), out.version,
            if out.ok { "BUILD COMPLETE" } else { "BUILD FAILED" }));
        r.push_str(&format!("Platform: {}   Configuration: {}
", out.platform.label(), out.config.as_str()));
        r.push_str(&format!("Duration: {}   Warnings: {}   Errors: {}
",
            clock(out.duration.as_secs()), out.warnings, out.errors));
        for s in crate::ops::run::Stage::ALL {
            if let Some(d) = out.stages[s.index()] {
                r.push_str(&format!("  {:<8}{}
", s.label(), clock(d.as_secs())));
            }
        }
        if let Some(z) = &out.zip { r.push_str(&format!("Output: {}
", z.display())); }
        if let Some(l) = &out.log { r.push_str(&format!("Log: {}
", l.display())); }
        for d in &self.build_log_diagnosis {
            r.push_str(&format!("
Cause: {}
Fix: {}
", d.explanation, d.fix));
        }
        if !out.error_samples.is_empty() {
            r.push_str("
First errors:
");
            for e in &out.error_samples { r.push_str(&format!("  {e}
")); }
        }
        r
    }

    fn project_name(&self) -> String {
        self.project_path.as_ref()
            .and_then(|p| p.file_stem())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// One of the three Ship choices: icon, title, sub-label.
fn ship_button(ui: &mut egui::Ui, icon: Icon, title: &str, sub: &str, primary_style: bool) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 58.0), egui::Sense::click());
    let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), T_FAST);

    let (fill, stroke, title_c) = if primary_style {
        (acc(30 + (hov * 16.0) as u8), acc(107), egui::Color32::from_rgb(214, 251, 247))
    } else {
        (mix(WELL, acc(24), hov), mix(LINE, acc(90), hov), TEXT)
    };
    let p = ui.painter();
    p.rect_filled(rect, egui::Rounding::same(13.0), fill);
    p.rect_stroke(rect, egui::Rounding::same(13.0), egui::Stroke::new(1.0, stroke));

    let icon_c = if primary_style { title_c } else { SOFT };
    paint_icon(p, rect.left_center() + egui::vec2(24.0, 0.0), icon, icon_c);
    p.text(rect.left_center() + egui::vec2(44.0, -8.0), egui::Align2::LEFT_CENTER,
           title, display(13.5), title_c);
    p.text(rect.left_center() + egui::vec2(44.0, 9.0), egui::Align2::LEFT_CENTER,
           sub, body(11.0), if primary_style { egui::Color32::from_rgb(143, 201, 196) } else { MUTED });

    if hov > 0.0 && hov < 1.0 { ui.ctx().request_repaint(); }
    resp.clicked()
}

/// The four-stage pipeline as exactly four equal cells across the full width.
///
/// It was four `ui.vertical` blocks with `add_space(6)` between them. Each
/// block was already `w / 4` wide and the row's own item spacing was added on
/// top, so the whole thing came out ~24pt wider than the card it sat in — and a
/// card grows to fit its widest child, so the card, and the prechecks strip
/// below it, pushed out over the rail. Cells are placed from explicit
/// rectangles, so the row can never be wider than the width it was given.
fn stage_columns(
    ui: &mut egui::Ui,
    height: f32,
    mut cell: impl FnMut(&mut egui::Ui, usize, f32),
) {
    const GAP: f32 = 6.0;
    let w = ui.available_width();
    let cell_w = ((w - 3.0 * GAP) / 4.0).max(0.0);
    let (row, _) = ui.allocate_exact_size(egui::vec2(w, height), egui::Sense::hover());
    for i in 0..4 {
        let x = row.min.x + i as f32 * (cell_w + GAP);
        let r = egui::Rect::from_min_size(egui::pos2(x, row.min.y), egui::vec2(cell_w, height));
        let mut child = ui.new_child(
            egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Min)));
        child.spacing_mut().item_spacing.y = 0.0;
        cell(&mut child, i, cell_w);
    }
}

pub fn divider(ui: &mut egui::Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, LINE));
}
