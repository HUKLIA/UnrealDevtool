//! Editor startup recovery. Inspection never writes; every repair is previewed,
//! backed up, and checked for changes since the preview before it is applied.
use crate::ops::preflight::{CheckItem, CheckStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use winreg::{RegKey, enums::*};

const CLASS: &str = "UnrealDevTool.ProjectFile";
const MAPS: &str = "/Script/EngineSettings.GameMapsSettings";
const RHI: &str = "/Script/WindowsTargetPlatform.WindowsTargetSettings";

fn check(status: CheckStatus, label: &str, detail: impl Into<String>) -> CheckItem {
    CheckItem {
        status,
        label: label.into(),
        detail: detail.into(),
    }
}

pub fn editor_exe(engine: &Path) -> PathBuf {
    let bin = engine.join("Engine/Binaries/Win64");
    if bin.join("UnrealEditor.exe").is_file() {
        bin.join("UnrealEditor.exe")
    } else {
        bin.join("UE4Editor.exe")
    }
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    let a = fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b = fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

/// The identifier must resolve to this install, including custom build aliases.
pub fn association_for(engine: &Path) -> Option<String> {
    if let Ok(k) =
        RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Epic Games\\Unreal Engine\\Builds")
    {
        for (id, _) in k.enum_values().flatten() {
            if let Ok(p) = k.get_value::<String, _>(&id)
                && same_path(Path::new(&p), engine)
            {
                return Some(id);
            }
        }
    }
    if let Some((a, b, _)) = crate::ops::engines::read_version(engine) {
        let id = format!("{a}.{b}");
        if crate::engine::find_engine_by_association(&id).is_some_and(|p| same_path(&p, engine)) {
            return Some(id);
        }
    }
    None
}

pub fn find_selector() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(root) = std::env::var_os(var) {
            candidates.push(
                PathBuf::from(root)
                    .join("Epic Games/Launcher/Engine/Binaries/Win64/UnrealVersionSelector.exe"),
            );
        }
    }
    if let Ok(k) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\EpicGames\\EpicGamesLauncher")
    {
        if let Ok(root) = k.get_value::<String, _>("AppDataPath") {
            candidates
                .push(PathBuf::from(root).join("Engine/Binaries/Win64/UnrealVersionSelector.exe"));
        }
    }
    // An existing registered selector can live outside the default folders.
    for class in ["Unreal.ProjectFile", CLASS] {
        if let Ok(k) =
            RegKey::predef(HKEY_CLASSES_ROOT).open_subkey(format!("{class}\\shell\\open\\command"))
        {
            if let Ok(command) = k.get_value::<String, _>("") {
                if let Some(path) = command_executable(&command) {
                    candidates.push(path);
                }
            }
        }
    }
    candidates.into_iter().find(|p| valid_selector(p))
}

pub fn valid_selector(path: &Path) -> bool {
    path.is_file()
        && path.file_name().is_some_and(|n| {
            let n = n.to_string_lossy();
            n.eq_ignore_ascii_case("UnrealVersionSelector.exe")
                || n.eq_ignore_ascii_case("UnrealVersionSelector-Win64-Shipping.exe")
        })
}

fn command_executable(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        Some(PathBuf::from(rest.split_once('"')?.0))
    } else {
        Some(PathBuf::from(command.split_whitespace().next()?))
    }
}

fn healthy_open_command(command: &str) -> bool {
    command_executable(command).is_some_and(|p| valid_selector(&p))
        && command.to_ascii_lowercase().contains("/editor")
        && command.contains("\"%1\"")
}

pub struct Inspection {
    pub items: Vec<CheckItem>,
    pub selector: Option<PathBuf>,
    pub user_choice: Option<String>,
}

pub fn inspect(project: Option<&Path>, engine: Option<&Path>) -> Inspection {
    let mut items = Vec::new();
    let selector = find_selector();
    items.push(check(if selector.is_some() { CheckStatus::Ok } else { CheckStatus::Warn }, "UnrealVersionSelector",
        selector.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "Not found. Browse to Epic's original selector or repair the Launcher installation. Direct opening below does not need it.".into())));
    let user_choice = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.uproject\\UserChoice").ok()
        .and_then(|k| k.get_value::<String, _>("ProgId").ok());
    let class = user_choice.clone().or_else(|| {
        RegKey::predef(HKEY_CLASSES_ROOT)
            .open_subkey(".uproject")
            .ok()
            .and_then(|k| k.get_value::<String, _>("").ok())
    });
    let command = class
        .as_ref()
        .and_then(|class| {
            RegKey::predef(HKEY_CLASSES_ROOT)
                .open_subkey(format!("{class}\\shell\\open\\command"))
                .ok()
        })
        .and_then(|k| k.get_value::<String, _>("").ok());
    let healthy = command.as_ref().is_some_and(|s| healthy_open_command(s));
    items.push(check(if healthy { CheckStatus::Ok } else { CheckStatus::Warn }, "Windows .uproject association",
        command.unwrap_or_else(|| "No usable open command. This matches the 'no app associated' or 'Invalid command line' symptom; it does not prove the project is damaged.".into())));
    if let Some(choice) = &user_choice {
        items.push(check(CheckStatus::Warn, "Windows default-app override", format!("UserChoice={choice}. Windows may prioritize this over a repaired registration. Use Default apps to select the Unreal project handler; this tool never edits the protected UserChoice hash.")));
    }
    if let Some(engine) = engine {
        let exe = editor_exe(engine);
        items.push(check(
            if exe.is_file() {
                CheckStatus::Ok
            } else {
                CheckStatus::Fail
            },
            "Editor executable",
            exe.display().to_string(),
        ));
    } else {
        items.push(check(
            CheckStatus::Fail,
            "Editor executable",
            "Choose the installed engine under Project setup & checks.",
        ));
    }
    if let Some(project) = project {
        match descriptor(project) {
            Err(e) => items.push(check(CheckStatus::Fail, "Project descriptor", e)),
            Ok(json) => {
                let requested = json
                    .get("EngineAssociation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let resolved = crate::engine::project_engine(project);
                let matches = engine
                    .zip(resolved.as_deref())
                    .is_some_and(|(a, b)| same_path(a, b));
                items.push(check(if matches { CheckStatus::Ok } else { CheckStatus::Warn }, "Project engine selection",
                    format!("Project asks for {}. {}", if requested.is_empty() { "a native/relative engine" } else { requested },
                        if matches { "The selected engine matches." } else { "The selected engine is not confirmed to match. Select the intended engine; opening with another version can convert assets." })));
                if json
                    .get("Modules")
                    .and_then(|v| v.as_array())
                    .is_some_and(|m| !m.is_empty())
                    && !project.parent().unwrap().join("Source").is_dir()
                {
                    items.push(check(CheckStatus::Fail, "C++ source missing", "The descriptor lists code modules but Source is missing. Restore source/plugins from the project owner; changing engine settings will not recreate code."));
                }
                if let Ok(text) =
                    fs::read_to_string(project.parent().unwrap().join("Config/DefaultEngine.ini"))
                {
                    if let Some(map) =
                        ini_value(&text, MAPS, "EditorStartupMap").filter(|s| !s.is_empty())
                    {
                        items.push(check(CheckStatus::Warn, "Startup map configured", format!("{map}. If the editor crashes while loading this map, preview clearing only EditorStartupMap below; game default maps are preserved.")));
                    }
                    if ini_value(&text, RHI, "DefaultGraphicsRHI")
                        .is_some_and(|s| s.contains("DX12"))
                    {
                        items.push(check(CheckStatus::Warn, "DirectX 12 configured", "For a GPU/D3D startup crash, try the temporary DX11 launch before changing project settings. This may disable DX12-only rendering features."));
                    }
                }
            }
        }
    } else {
        items.push(check(CheckStatus::Warn, "Project", "Pick a .uproject to check its engine and startup settings. Windows association repair is available without a project."));
    }
    Inspection {
        items,
        selector,
        user_choice,
    }
}

fn descriptor(project: &Path) -> Result<serde_json::Value, String> {
    if !project.is_file()
        || project
            .extension()
            .is_none_or(|s| !s.eq_ignore_ascii_case("uproject"))
    {
        return Err("Choose an existing .uproject file.".into());
    }
    let text = fs::read_to_string(project).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("Invalid .uproject JSON: {e}"))?;
    if !json.is_object() {
        return Err("The .uproject must contain a JSON object.".into());
    }
    Ok(json)
}

fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    let mut value = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            inside = line.trim_matches(['[', ']']).eq_ignore_ascii_case(section);
        } else if inside
            && let Some((k, v)) = line.split_once('=')
            && k.trim().eq_ignore_ascii_case(key)
        {
            value = Some(v.trim().to_string());
        }
    }
    value
}

/// Replace all duplicate scalar occurrences, since Unreal can use the last one.
fn set_scalar(text: &str, section: &str, key: &str, value: &str) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut inside = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t.trim_matches(['[', ']']).eq_ignore_ascii_case(section);
        }
        if inside
            && t.split_once('=')
                .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        {
            continue;
        }
        lines.push(line);
    }
    crate::ops::doctor::ini_set(&lines.join(nl), section, key, value)
}

#[derive(Clone, Copy)]
pub enum FileFix {
    EngineAssociation,
    ClearStartupMap,
    UseDx11,
    ResetEditorPreferences,
}

#[derive(Clone, Debug)]
pub struct FilePlan {
    pub project: PathBuf,
    pub path: PathBuf,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub summary: String,
}

fn confined_file(project: &Path, path: &Path) -> Result<(), String> {
    let root = project
        .parent()
        .ok_or("Project folder missing")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let actual = path.canonicalize().map_err(|e| e.to_string())?;
    if !actual.starts_with(root) {
        return Err("Repair target resolves outside the project folder (link/junction).".into());
    }
    Ok(())
}

pub fn plan_file(project: &Path, engine: Option<&Path>, fix: FileFix) -> Result<FilePlan, String> {
    let mut json = descriptor(project)?;
    let root = project.parent().ok_or("Project folder missing")?;
    let path = match fix {
        FileFix::EngineAssociation => project.to_path_buf(),
        FileFix::ClearStartupMap | FileFix::UseDx11 => root.join("Config/DefaultEngine.ini"),
        FileFix::ResetEditorPreferences => ["WindowsEditor", "Windows"]
            .into_iter()
            .map(|p| {
                root.join("Saved/Config")
                    .join(p)
                    .join("EditorPerProjectUserSettings.ini")
            })
            .find(|p| p.is_file())
            .ok_or("No generated editor preferences exist for this project.")?,
    };
    confined_file(project, &path)?;
    let before = fs::read(&path).map_err(|e| e.to_string())?;
    let text = std::str::from_utf8(&before).map_err(
        |_| "Settings are not UTF-8; automatic repair refused to avoid corrupting encoding.",
    )?;
    let (after, summary) = match fix {
        FileFix::EngineAssociation => {
            let id = association_for(engine.ok_or("Choose an installed engine first")?).ok_or("Selected engine is not registered with a resolvable version/alias. Use Epic Launcher or register the source engine before changing the project.")?;
            json["EngineAssociation"] = id.clone().into();
            let mut body = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
            body.push('\n');
            if text.contains("\r\n") { body = body.replace('\n', "\r\n"); }
            (body, format!("Set EngineAssociation to {id}. Opening with a different engine can convert assets; use a project copy or commit first."))
        }
        FileFix::ClearStartupMap => (set_scalar(text, MAPS, "EditorStartupMap", ""), "Clear only EditorStartupMap. GameDefaultMap and assets are preserved.".into()),
        FileFix::UseDx11 => (set_scalar(text, RHI, "DefaultGraphicsRHI", "DefaultGraphicsRHI_DX11"), "Set Windows DefaultGraphicsRHI to DX11. DX12-only features may be unavailable.".into()),
        FileFix::ResetEditorPreferences => (String::new(), "Reset generated editor preferences for this project (layout and per-project editor choices). Project Config, assets, plugins and source are preserved.".into()),
    };
    Ok(FilePlan {
        project: project.to_path_buf(),
        path,
        before,
        after: after.into_bytes(),
        summary,
    })
}

#[derive(Clone, Debug)]
pub struct FileUndo {
    pub plan: FilePlan,
    pub backup: PathBuf,
}

pub fn apply_file(plan: &FilePlan) -> Result<FileUndo, String> {
    confined_file(&plan.project, &plan.path)?;
    if fs::read(&plan.path).map_err(|e| e.to_string())? != plan.before {
        return Err("File changed since preview. Scan and preview again.".into());
    }
    if plan.before == plan.after {
        return Err("This setting already matches the proposed value.".into());
    }
    let backup = PathBuf::from(format!(
        "{}.devtool-repair-{}.bak",
        plan.path.display(),
        unique_id()
    ));
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
        .map_err(|e| e.to_string())?;
    file.write_all(&plan.before)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    if let Err(e) = fs::write(&plan.path, &plan.after) {
        let recovery = fs::write(&plan.path, &plan.before);
        return Err(format!(
            "Write failed: {e}. Restore: {recovery:?}. Backup: {}",
            backup.display()
        ));
    }
    Ok(FileUndo {
        plan: plan.clone(),
        backup,
    })
}

pub fn restore_file(undo: &FileUndo) -> Result<(), String> {
    confined_file(&undo.plan.project, &undo.plan.path)?;
    if fs::read(&undo.plan.path).map_err(|e| e.to_string())? != undo.plan.after {
        return Err("File changed after repair (possibly by the editor). Restore refused; review the backup manually rather than overwrite newer work.".into());
    }
    let bytes = fs::read(&undo.backup).map_err(|e| e.to_string())?;
    if bytes != undo.plan.before {
        return Err("Backup contents changed; restore refused.".into());
    }
    fs::write(&undo.plan.path, bytes).map_err(|e| e.to_string())
}

pub fn unique_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub fn launch_args(project: &Path, dx11: bool) -> Vec<String> {
    let mut args = vec![project.display().to_string(), "-log".into()];
    if dx11 {
        args.push("-dx11".into());
    }
    args
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryChange {
    pub path: String,
    pub before: Option<String>,
    pub after: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssociationPlan {
    pub changes: Vec<RegistryChange>,
}

trait RegistryStore {
    fn read(&self, path: &str) -> Result<Option<String>, String>;
    fn write(&mut self, path: &str, value: Option<&str>) -> Result<(), String>;
}
pub fn registry_value_name(path: &str) -> &'static str {
    if path == "Software\\Classes\\.uproject\\OpenWithProgids" {
        CLASS
    } else {
        ""
    }
}

struct UserClasses;
impl RegistryStore for UserClasses {
    fn read(&self, path: &str) -> Result<Option<String>, String> {
        let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey(path) {
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let name = registry_value_name(path);
        match key.get_raw_value(name) {
            Ok(v) if v.vtype == REG_SZ => key.get_value(name).map(Some).map_err(|e| e.to_string()),
            Ok(_) => Err(format!(
                "Unexpected registry value type at {path}; automatic repair refused."
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn write(&mut self, path: &str, value: Option<&str>) -> Result<(), String> {
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(path)
            .map_err(|e| e.to_string())?;
        let name = registry_value_name(path);
        match value {
            Some(value) => k.set_value(name, &value).map_err(|e| e.to_string()),
            None => match k.delete_value(name) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            },
        }
    }
}

fn association_values(selector: &Path) -> Vec<(String, String)> {
    let root = format!("Software\\Classes\\{CLASS}");
    vec![
        ("Software\\Classes\\.uproject".into(), CLASS.into()),
        (
            "Software\\Classes\\.uproject\\OpenWithProgids".into(),
            String::new(),
        ),
        (root.clone(), "Unreal Engine Project".into()),
        (
            format!("{root}\\DefaultIcon"),
            format!("\"{}\",0", selector.display()),
        ),
        (
            format!("{root}\\shell\\open\\command"),
            format!("\"{}\" /editor \"%1\"", selector.display()),
        ),
        (
            format!("{root}\\shell\\rungenproj"),
            "Generate Visual Studio project files".into(),
        ),
        (
            format!("{root}\\shell\\rungenproj\\command"),
            format!("\"{}\" /projectfiles \"%1\"", selector.display()),
        ),
        (
            format!("{root}\\shell\\switchversion"),
            "Switch Unreal Engine version...".into(),
        ),
        (
            format!("{root}\\shell\\switchversion\\command"),
            format!("\"{}\" /switchversion \"%1\"", selector.display()),
        ),
    ]
}

pub fn plan_association(selector: &Path) -> Result<AssociationPlan, String> {
    if !valid_selector(selector) {
        return Err("Select an existing original UnrealVersionSelector executable. No executable is downloaded or copied.".into());
    }
    let store = UserClasses;
    let changes = association_values(selector)
        .into_iter()
        .map(|(path, after)| {
            Ok(RegistryChange {
                before: store.read(&path)?,
                path,
                after,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(AssociationPlan { changes })
}

fn validate_scope(plan: &AssociationPlan) -> Result<(), String> {
    let allowed: Vec<_> = association_values(Path::new("unused"))
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    if plan.changes.len() != allowed.len()
        || plan.changes.iter().zip(allowed).any(|(c, p)| c.path != p)
    {
        return Err("Association backup contains unexpected registry paths; refused.".into());
    }
    Ok(())
}

fn transition(
    store: &mut impl RegistryStore,
    plan: &AssociationPlan,
    restore: bool,
) -> Result<(), String> {
    validate_scope(plan)?;
    for c in &plan.changes {
        let expected = if restore {
            Some(c.after.clone())
        } else {
            c.before.clone()
        };
        if store.read(&c.path)? != expected {
            return Err(format!(
                "Registry changed since preview/repair at {}. Refresh before changing it.",
                c.path
            ));
        }
    }
    for (index, c) in plan.changes.iter().enumerate() {
        let value = if restore {
            c.before.as_deref()
        } else {
            Some(c.after.as_str())
        };
        if let Err(e) = store.write(&c.path, value) {
            let mut failures = Vec::new();
            for previous in plan.changes[..index].iter().rev() {
                let original = if restore {
                    Some(previous.after.as_str())
                } else {
                    previous.before.as_deref()
                };
                if let Err(err) = store.write(&previous.path, original) {
                    failures.push(err);
                }
            }
            return Err(format!(
                "Registry write failed: {e}. Rollback errors: {failures:?}. Backup retained."
            ));
        }
    }
    Ok(())
}

pub fn apply_association(plan: &AssociationPlan) -> Result<PathBuf, String> {
    validate_scope(plan)?;
    let dir = crate::config::config_dir()
        .ok_or("Settings directory unavailable")?
        .join("repairs");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("association-{}.json", unique_id()));
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(plan).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    transition(&mut UserClasses, plan, false)?;
    notify_associations();
    Ok(path)
}

pub fn read_association_backup(path: &Path) -> Result<AssociationPlan, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 128 * 1024 {
        return Err("Association backup exceeds 128 KiB.".into());
    }
    let plan: AssociationPlan = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    validate_scope(&plan)?;
    Ok(plan)
}

pub fn restore_association(path: &Path, preview: &AssociationPlan) -> Result<(), String> {
    let plan = read_association_backup(path)?;
    if &plan != preview {
        return Err("Association backup changed since preview. Open and review it again.".into());
    }
    transition(&mut UserClasses, &plan, true)?;
    notify_associations();
    Ok(())
}

fn notify_associations() {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHChangeNotify(
            event: i32,
            flags: u32,
            first: *const std::ffi::c_void,
            second: *const std::ffi::c_void,
        );
    }
    // SHCNE_ASSOCCHANGED with SHCNF_IDLIST; no pointers are required.
    unsafe {
        SHChangeNotify(0x08000000, 0, std::ptr::null(), std::ptr::null());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fixture {
        root: PathBuf,
        project: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "devtool-editor-repair-{}-{}",
                std::process::id(),
                unique_id()
            ));
            fs::create_dir_all(root.join("Config")).unwrap();
            let project = root.join("Game with spaces.uproject");
            fs::write(&project, "{\"FileVersion\":3,\"EngineAssociation\":\"5.7\",\"Plugins\":[{\"Name\":\"KeepMe\",\"Enabled\":true}]}").unwrap();
            Self { root, project }
        }
        fn ini(&self, body: &str) {
            fs::write(self.root.join("Config/DefaultEngine.ini"), body).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn clears_editor_map_without_changing_game_map_and_restores_exact_bytes() {
        let f = Fixture::new();
        f.ini(";keep comment\r\n[/Script/EngineSettings.GameMapsSettings]\r\nGameDefaultMap=/Game/Keep\r\nEditorStartupMap=/Game/Broken\r\nEditorStartupMap=/Game/AlsoBroken\r\n");
        let plan = plan_file(&f.project, None, FileFix::ClearStartupMap).unwrap();
        let text = String::from_utf8(plan.after.clone()).unwrap();
        assert_eq!(
            ini_value(&text, MAPS, "EditorStartupMap"),
            Some(String::new())
        );
        assert!(text.contains("GameDefaultMap=/Game/Keep\r\n"));
        assert_eq!(text.matches("EditorStartupMap=").count(), 1);
        let undo = apply_file(&plan).unwrap();
        assert_eq!(fs::read(&undo.backup).unwrap(), plan.before);
        restore_file(&undo).unwrap();
        assert_eq!(fs::read(&plan.path).unwrap(), plan.before);
    }

    #[test]
    fn stale_preview_and_changed_file_restore_are_refused() {
        let f = Fixture::new();
        f.ini(";original\n");
        let plan = plan_file(&f.project, None, FileFix::UseDx11).unwrap();
        fs::write(&plan.path, ";new work").unwrap();
        assert!(
            apply_file(&plan)
                .unwrap_err()
                .contains("changed since preview")
        );
        assert_eq!(fs::read_to_string(&plan.path).unwrap(), ";new work");
        let plan = plan_file(&f.project, None, FileFix::UseDx11).unwrap();
        let undo = apply_file(&plan).unwrap();
        fs::write(&plan.path, ";editor wrote something new").unwrap();
        assert!(
            restore_file(&undo)
                .unwrap_err()
                .contains("changed after repair")
        );
    }

    #[test]
    fn renderer_change_preserves_other_settings_and_invalid_json_is_rejected() {
        let f = Fixture::new();
        f.ini("[Other]\nKeep=Yes\n[/Script/WindowsTargetPlatform.WindowsTargetSettings]\nDefaultGraphicsRHI=DefaultGraphicsRHI_DX12\n");
        let plan = plan_file(&f.project, None, FileFix::UseDx11).unwrap();
        let text = String::from_utf8(plan.after).unwrap();
        assert!(text.contains("Keep=Yes"));
        assert_eq!(
            ini_value(&text, RHI, "DefaultGraphicsRHI").as_deref(),
            Some("DefaultGraphicsRHI_DX11")
        );
        fs::write(&f.project, "not json").unwrap();
        assert!(plan_file(&f.project, None, FileFix::UseDx11).is_err());
    }

    #[test]
    fn reset_only_touches_generated_editor_preferences() {
        let f = Fixture::new();
        let path = f
            .root
            .join("Saved/Config/WindowsEditor/EditorPerProjectUserSettings.ini");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::create_dir_all(f.root.join("Saved/Autosaves")).unwrap();
        fs::write(f.root.join("Saved/Autosaves/Keep.umap"), "precious").unwrap();
        fs::write(&path, "Layout=broken").unwrap();
        let plan = plan_file(&f.project, None, FileFix::ResetEditorPreferences).unwrap();
        let undo = apply_file(&plan).unwrap();
        assert!(fs::read(&path).unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(f.root.join("Saved/Autosaves/Keep.umap")).unwrap(),
            "precious"
        );
        restore_file(&undo).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "Layout=broken");
    }

    #[test]
    fn direct_launch_keeps_spaced_project_as_one_argument() {
        let project = Path::new("C:/Projects/My Game/Game.uproject");
        assert_eq!(
            launch_args(project, false),
            ["C:/Projects/My Game/Game.uproject", "-log"]
        );
        assert_eq!(
            launch_args(project, true),
            ["C:/Projects/My Game/Game.uproject", "-log", "-dx11"]
        );
    }

    #[test]
    fn missing_selector_and_unquoted_command_are_not_accepted() {
        assert!(!valid_selector(Path::new(
            "C:/missing/UnrealVersionSelector.exe"
        )));
        let command =
            "\"C:\\Program Files (x86)\\Epic Games\\UnrealVersionSelector.exe\" /editor \"%1\"";
        assert_eq!(
            command_executable(command).unwrap(),
            PathBuf::from("C:\\Program Files (x86)\\Epic Games\\UnrealVersionSelector.exe")
        );
        assert_ne!(
            command_executable(&command.replace('"', "")).unwrap(),
            command_executable(command).unwrap()
        );
    }

    #[test]
    fn association_case_rejects_missing_selector_or_missing_quoted_project_argument() {
        let f = Fixture::new();
        let selector = f.root.join("UnrealVersionSelector.exe");
        fs::write(&selector, b"fixture only; never executed").unwrap();
        let correct = format!("\"{}\" /editor \"%1\"", selector.display());
        assert!(healthy_open_command(&correct));
        assert!(!healthy_open_command(&correct.replace("\"%1\"", "%1")));
        assert!(!healthy_open_command(
            &correct.replace("/editor", "/register")
        ));
        fs::remove_file(selector).unwrap();
        assert!(!healthy_open_command(&correct));
    }

    #[test]
    fn targets_outside_project_are_refused_even_with_matching_bytes() {
        let f = Fixture::new();
        let other = Fixture::new();
        other.ini("do not overwrite");
        let path = other.root.join("Config/DefaultEngine.ini");
        let plan = FilePlan {
            project: f.project.clone(),
            path: path.clone(),
            before: b"do not overwrite".to_vec(),
            after: b"changed".to_vec(),
            summary: String::new(),
        };
        assert!(apply_file(&plan).unwrap_err().contains("outside"));
        assert_eq!(fs::read(path).unwrap(), b"do not overwrite");
    }

    #[test]
    fn repeated_repairs_keep_unique_backups() {
        let f = Fixture::new();
        f.ini(";original\n");
        let first = apply_file(&plan_file(&f.project, None, FileFix::UseDx11).unwrap()).unwrap();
        let second =
            apply_file(&plan_file(&f.project, None, FileFix::ClearStartupMap).unwrap()).unwrap();
        assert_ne!(first.backup, second.backup);
        assert_eq!(fs::read(&first.backup).unwrap(), b";original\n");
    }

    #[test]
    fn malformed_settings_encoding_is_not_overwritten() {
        let f = Fixture::new();
        fs::write(f.root.join("Config/DefaultEngine.ini"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(plan_file(&f.project, None, FileFix::UseDx11).is_err());
    }

    #[derive(Default)]
    struct FakeRegistry {
        values: BTreeMap<String, String>,
        writes: usize,
        fail_at: Option<usize>,
    }
    impl RegistryStore for FakeRegistry {
        fn read(&self, p: &str) -> Result<Option<String>, String> {
            Ok(self.values.get(p).cloned())
        }
        fn write(&mut self, p: &str, v: Option<&str>) -> Result<(), String> {
            self.writes += 1;
            if self.fail_at == Some(self.writes) {
                return Err("simulated permission failure".into());
            }
            if let Some(v) = v {
                self.values.insert(p.into(), v.into());
            } else {
                self.values.remove(p);
            }
            Ok(())
        }
    }
    fn registry_fixture() -> (AssociationPlan, FakeRegistry) {
        let mut store = FakeRegistry::default();
        store
            .values
            .insert("Software\\Classes\\.uproject".into(), "Old.Handler".into());
        let changes = association_values(Path::new("C:/Epic Games/UnrealVersionSelector.exe"))
            .into_iter()
            .map(|(path, after)| RegistryChange {
                before: store.values.get(&path).cloned(),
                path,
                after,
            })
            .collect();
        (AssociationPlan { changes }, store)
    }

    #[test]
    fn association_repair_and_restore_preserve_existing_handler() {
        let (plan, mut store) = registry_fixture();
        let original = store.values.clone();
        transition(&mut store, &plan, false).unwrap();
        assert!(store.values.values().any(|v| v.contains("/editor \"%1\"")));
        assert!(
            store
                .values
                .values()
                .any(|v| v.contains("/projectfiles \"%1\""))
        );
        transition(&mut store, &plan, true).unwrap();
        assert_eq!(store.values, original);
    }

    #[test]
    fn association_write_failure_rolls_back_all_applied_values() {
        let (plan, mut store) = registry_fixture();
        let original = store.values.clone();
        store.fail_at = Some(4);
        assert!(
            transition(&mut store, &plan, false)
                .unwrap_err()
                .contains("Rollback errors: []")
        );
        assert_eq!(store.values, original);
    }

    #[test]
    fn protected_userchoice_and_outside_registry_paths_are_never_written() {
        let (mut plan, mut store) = registry_fixture();
        plan.changes[0].path = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.uproject\\UserChoice".into();
        assert!(transition(&mut store, &plan, false).is_err());
        assert_eq!(store.writes, 0);
    }

    #[test]
    fn changed_associations_are_not_overwritten_during_apply_or_restore() {
        let (plan, mut store) = registry_fixture();
        store
            .values
            .insert(plan.changes[0].path.clone(), "New.Handler".into());
        assert!(transition(&mut store, &plan, false).is_err());
        assert_eq!(store.writes, 0);
        let (plan, mut store) = registry_fixture();
        transition(&mut store, &plan, false).unwrap();
        store
            .values
            .insert(plan.changes[0].path.clone(), "New.Handler".into());
        assert!(transition(&mut store, &plan, true).is_err());
        assert_eq!(store.values[&plan.changes[0].path], "New.Handler");
    }

    #[test]
    fn backup_preview_rejects_unexpected_paths_and_changed_backup_files() {
        let f = Fixture::new();
        let path = f.root.join("backup.json");
        let (mut plan, _) = registry_fixture();
        fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
        let preview = read_association_backup(&path).unwrap();
        plan.changes[0].before = Some("Changed.After.Preview".into());
        fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
        // Refuses before inspecting or mutating the real registry.
        assert!(
            restore_association(&path, &preview)
                .unwrap_err()
                .contains("changed since preview")
        );
        plan.changes[0].path = "Unexpected\\Registry\\Path".into();
        fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
        assert!(read_association_backup(&path).is_err());
        fs::write(&path, vec![0; 128 * 1024 + 1]).unwrap();
        assert!(
            read_association_backup(&path)
                .unwrap_err()
                .contains("128 KiB")
        );
    }
}
