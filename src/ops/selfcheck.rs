use crate::ops::preflight::{CheckItem, CheckStatus};

/// Checks about the DevTool app itself — its own install, config, and
/// leftover state from updates — as opposed to `preflight::run_checks`,
/// which is about the *Unreal project/engine* setup. Pure disk reads, no
/// network — safe to call directly on the UI thread.
pub fn run_checks() -> Vec<CheckItem> {
    let mut items = vec![CheckItem {
        status: CheckStatus::Ok,
        label:  "Version".into(),
        detail: env!("CARGO_PKG_VERSION").to_string(),
    }];

    match std::env::current_exe() {
        Ok(exe) => {
            items.push(CheckItem {
                status: CheckStatus::Ok, label: "Install location".into(),
                detail: exe.display().to_string(),
            });
            if let Some(dir) = exe.parent() {
                items.push(install_folder_check(dir));
            }
        }
        Err(e) => items.push(CheckItem {
            status: CheckStatus::Fail, label: "Install location".into(),
            detail: format!("Could not resolve: {e}"),
        }),
    }

    items.push(leftover_binary_check());
    items.push(config_folder_check());

    items
}

fn install_folder_check(dir: &std::path::Path) -> CheckItem {
    if crate::ops::update::dir_is_writable(dir) {
        return CheckItem {
            status: CheckStatus::Ok, label: "Install folder writable".into(),
            detail: "Self-update can install new versions here.".into(),
        };
    }
    let hint = if dir.to_string_lossy().to_ascii_lowercase().contains("program files") {
        " — it's under Program Files, which needs administrator rights to write to"
    } else {
        ""
    };
    CheckItem {
        status: CheckStatus::Warn, label: "Install folder not writable".into(),
        detail: format!(
            "Self-update will fail here{hint}. Run as administrator, or move the app to a \
             different folder (e.g. Documents or a dedicated tools folder)."
        ),
    }
}

fn leftover_binary_check() -> CheckItem {
    match crate::ops::update::leftover_old_binary_size() {
        Some(bytes) => CheckItem {
            status: CheckStatus::Warn, label: "Leftover update file".into(),
            detail: format!(
                "unreal_devtool_old.exe ({:.1} MB) wasn't cleaned up after the last update. \
                 Usually harmless (it's cleaned up automatically on the next successful \
                 startup), but you can remove it now below.",
                bytes as f64 / 1_048_576.0
            ),
        },
        None => CheckItem {
            status: CheckStatus::Ok, label: "Leftover update file".into(), detail: "None found.".into(),
        },
    }
}

fn config_folder_check() -> CheckItem {
    let Some(dir) = crate::config::config_dir() else {
        return CheckItem {
            status: CheckStatus::Fail, label: "Config folder".into(),
            detail: "Could not resolve %APPDATA%.".into(),
        };
    };
    let _ = std::fs::create_dir_all(&dir);
    if crate::ops::update::dir_is_writable(&dir) {
        CheckItem {
            status: CheckStatus::Ok, label: "Config folder".into(), detail: dir.display().to_string(),
        }
    } else {
        CheckItem {
            status: CheckStatus::Fail, label: "Config folder not writable".into(),
            detail: format!(
                "{} — settings (project path, engine override, etc.) won't save.", dir.display()
            ),
        }
    }
}

/// GitHub reachability (needed for update checks) — a real network call, so
/// callers must run this on a background thread, not the UI thread.
pub fn github_reachable() -> CheckItem {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(5))
        .build();
    match agent.get("https://api.github.com").call() {
        Ok(_) => CheckItem {
            status: CheckStatus::Ok, label: "GitHub connectivity".into(),
            detail: "Reachable — update checks should work.".into(),
        },
        Err(e) => CheckItem {
            status: CheckStatus::Warn, label: "GitHub connectivity".into(),
            detail: format!(
                "Could not reach GitHub ({e}) — update checks will silently fail until this is \
                 reachable (check your network/firewall/proxy)."
            ),
        },
    }
}
