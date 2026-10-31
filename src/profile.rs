//! Isolated DCS.Tablet profile with an ownership manifest and reversible writes.
use crate::filesystem;
use serde_json::{Value, json};
use std::io::Write;
use std::{
    fs,
    path::{Path, PathBuf},
};
pub const BRIDGE: &str = include_str!("../dcs/DCSTabletControls.lua");
const MARKER: &str = ".dcs-tablet-profile.json";
#[derive(serde::Deserialize, serde::Serialize)]
struct ManagedFile {
    name: String,
    after: String,
}
#[derive(serde::Deserialize, serde::Serialize)]
struct Manifest {
    version: u32,
    width: u32,
    height: u32,
    files: Vec<ManagedFile>,
}
fn reserved_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}
impl Manifest {
    fn load(path: &Path) -> Result<Self, String> {
        let manifest: Self = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Invalid profile manifest: {e}"))?;
        if !(1..=2).contains(&manifest.version) || manifest.width == 0 || manifest.height == 0 {
            return Err("Unsupported profile manifest".into());
        }
        let mut names = std::collections::HashSet::new();
        for file in &manifest.files {
            let name = file.name.replace('\\', "/");
            let known = matches!(
                name.as_str(),
                "Config/options.lua"
                    | "Config/MonitorSetup/DCSTablet.lua"
                    | "Scripts/DCSTabletControls.lua"
                    | "Scripts/Export.lua"
            ) || name.starts_with("Config/Input/");
            if !known
                || name.contains(':')
                || name.split('/').any(|c| {
                    c.is_empty()
                        || c == "."
                        || c == ".."
                        || c.ends_with(['.', ' '])
                        || reserved_name(c)
                })
                || !names.insert(name.to_lowercase())
            {
                return Err("Unsafe or duplicate profile manifest filename".into());
            }
        }
        for required in [
            "Config/options.lua",
            "Config/MonitorSetup/DCSTablet.lua",
            "Scripts/DCSTabletControls.lua",
            "Scripts/Export.lua",
        ] {
            if !names.contains(&required.to_lowercase()) {
                return Err("Incomplete profile manifest".into());
            }
        }
        Ok(manifest)
    }
}
pub fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    write_atomic(
        path,
        &serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
}
fn unique_suffix() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    filesystem::reject_shared_file(path)?;
    let existed = path.exists();
    let name = path
        .file_name()
        .ok_or("Missing destination filename")?
        .to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.dcs-tablet-{}.tmp", unique_suffix()));
    // Exclusive creation never truncates an unrelated .tmp file or follows a link.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if !existed {
            return filesystem::move_new(&temporary, path);
        }
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{REPLACE_FILE_FLAGS, ReplaceFileW},
            core::PCWSTR,
        };
        let wide = |p: &Path| {
            p.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let destination = wide(path);
        let source = wide(&temporary);
        // SAFETY: both terminated buffers remain live through atomic replacement.
        unsafe {
            ReplaceFileW(
                PCWSTR(destination.as_ptr()),
                PCWSTR(source.as_ptr()),
                None,
                REPLACE_FILE_FLAGS(0),
                None,
                None,
            )
        }
        .map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub fn root(settings: &Value) -> Result<PathBuf, String> {
    Ok(PathBuf::from(
        settings["saved_games"]
            .as_str()
            .ok_or("Missing saved_games")?,
    )
    .join("DCS.Tablet"))
}
pub fn prepare(settings: &Value, primary: &crate::display::Display) -> Result<(), String> {
    let root = root(settings)?;
    filesystem::reject_link(&root)?;
    let marker = filesystem::profile_path(&root, Path::new(MARKER))?;
    if marker.exists() {
        let mut manifest = Manifest::load(&marker)?;
        if manifest.width != primary.width || manifest.height != primary.height {
            return Err(
                "Primary resolution changed. Run uninstall-profile, then setup again.".into(),
            );
        }
        // Validate every managed file before writing. Options/input are independent user copies.
        // The old manifest remains until all writes finish. A retry accepts either the old
        // owned content or the current embedded bridge after an interrupted upgrade.
        for entry in &manifest.files {
            filesystem::profile_path(&root, Path::new(&entry.name))?;
            let name = entry.name.replace('\\', "/");
            if name == "Config/options.lua" || name.starts_with("Config/Input/") {
                continue;
            }
            let current = fs::read_to_string(root.join(&entry.name)).map_err(|e| e.to_string())?;
            let upgraded_bridge = name == "Scripts/DCSTabletControls.lua" && current == BRIDGE;
            if current != entry.after && !upgraded_bridge {
                return Err(format!(
                    "Managed file changed: {}. Preserve your changes before reinstalling.",
                    root.join(&entry.name).display()
                ));
            }
        }
        for entry in &mut manifest.files {
            if entry.name.replace('\\', "/") == "Scripts/DCSTabletControls.lua"
                && entry.after != BRIDGE
            {
                write_atomic(&root.join(&entry.name), BRIDGE.as_bytes())?;
                entry.after = BRIDGE.to_owned();
            }
        }
        manifest.version = 2;
        write_json(
            &marker,
            &serde_json::to_value(manifest).map_err(|e| e.to_string())?,
        )?;
        return Ok(());
    }
    if root.exists() {
        return Err(format!(
            "{} already exists without DCS Tablet ownership. Preserve this folder and rename it to an unused backup name before setup.",
            root.display()
        ));
    }
    let normal = PathBuf::from(
        settings["source_profile"]
            .as_str()
            .ok_or("Missing source_profile")?,
    );
    if normal
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("DCS.Tablet"))
    {
        return Err("Source profile must differ from DCS.Tablet".into());
    }
    let options = fs::read_to_string(normal.join("Config/options.lua"))
        .map_err(|e| format!("Cannot read source profile options: {e}"))?;
    let width = primary.width;
    let height = primary.height.max(512);
    let total = width + 1024;
    let options = format!(
        "{options}\n-- DCS Tablet: dedicated non-VR display configuration\noptions.graphics.multiMonitorSetup = 'dcstablet'\noptions.graphics.width = {total}\noptions.graphics.height = {height}\noptions.graphics.aspect = {total}/{height}\noptions.graphics.fullScreen = false\noptions.graphics.mirror_use_DCS_resolution = false\noptions.VR = options.VR or {{}}\noptions.VR.enable = false\n"
    );
    let monitor = format!(
        "_ = function(p) return p end\nname = _('DCS Tablet')\nDescription = 'Physical cockpit with two DDIs on an extended virtual display'\nViewports = {{ Center = {{ x=0; y=0; width={width}; height={}; viewDx=0; viewDy=0; aspect={width}/{}; }} }}\nLEFT_MFCD = {{ x={width}; y=0; width=512; height=512; }}\nRIGHT_MFCD = {{ x={}; y=0; width=512; height=512; }}\nUIMainView = Viewports.Center\nGU_MAIN_VIEWPORT = Viewports.Center\n",
        primary.height,
        primary.height,
        width + 512
    );
    let mut files = vec![
        json!({"name":"Config/options.lua","after":options}),
        json!({"name":"Config/MonitorSetup/DCSTablet.lua","after":monitor}),
        json!({"name":"Scripts/DCSTabletControls.lua","after":BRIDGE}),
        json!({"name":"Scripts/Export.lua","after":"-- DCS Tablet controls (managed)\npcall(function() dofile(require('lfs').writedir()..'Scripts/DCSTabletControls.lua') end)\n"}),
    ];
    // Retain input bindings as independent copies. No shared writable junctions or authentication files.
    collect_text_files(
        &normal.join("Config/Input"),
        Path::new("Config/Input"),
        &mut files,
    )?;
    // Claim a new directory exclusively. Never merge into a folder created after
    // the initial existence check (or replace an unowned profile).
    fs::create_dir(&root)
        .map_err(|e| format!("Cannot create an independent DCS.Tablet profile: {e}"))?;
    // Save the intended writes before applying them, so partial setup can be inspected/recovered.
    write_json(
        &marker,
        &json!({"version":2,"width":width,"height":primary.height,"files":files}),
    )?;
    for entry in &files {
        let path = root.join(entry["name"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(entry["after"].as_str().unwrap().as_bytes())
            .map_err(|e| e.to_string())?;
    }
    println!("Created isolated profile: {}", root.display());
    Ok(())
}
fn collect_text_files(
    source: &Path,
    relative: &Path,
    files: &mut Vec<Value>,
) -> Result<(), String> {
    filesystem::reject_link(source)?;
    if !source.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        filesystem::reject_link(&entry.path())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let name = relative.join(entry.file_name());
        if kind.is_dir() {
            collect_text_files(&entry.path(), &name, files)?;
        } else if kind.is_file() {
            let content = fs::read_to_string(entry.path()).map_err(|e| e.to_string())?;
            files.push(json!({"name":name.to_string_lossy(),"after":content}));
        } else {
            return Err(format!(
                "Source input profile contains a link: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}
/// Removal is an atomic archive, not deletion. Missions, credentials, edited
/// bindings, and DCS-generated files remain together in a same-volume backup.
pub fn uninstall(settings: &Value) -> Result<PathBuf, String> {
    let root = root(settings)?;
    filesystem::reject_link(&root)?;
    let marker = filesystem::profile_path(&root, Path::new(MARKER))?;
    let manifest = Manifest::load(&marker)?;
    for entry in &manifest.files {
        filesystem::profile_path(&root, Path::new(&entry.name))?;
    }
    let archive = root.with_file_name(format!("DCS.Tablet.backup-{}", unique_suffix()));
    filesystem::move_new(&root, &archive)?;
    println!(
        "Profile archived without deleting files: {}",
        archive.display()
    );
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, Value, crate::display::Display) {
        let dir = std::env::temp_dir().join(format!(
            "dcs-tablet-profile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let normal = dir.join("DCS");
        fs::create_dir_all(normal.join("Config/Input/FA-18C_hornet/joystick")).unwrap();
        fs::write(
            normal.join("Config/options.lua"),
            "options = { graphics = { width = 1920, height = 1080 }, VR = { enable = true } }\n",
        )
        .unwrap();
        fs::write(
            normal.join("Config/Input/FA-18C_hornet/joystick/test.diff.lua"),
            "return {}\n",
        )
        .unwrap();
        let settings = json!({"saved_games":dir,"source_profile":normal});
        let display = crate::display::Display {
            device: "test".into(),
            description: "test".into(),
            active: true,
            primary: true,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            frequency: 60,
        };
        (dir, settings, display)
    }
    #[test]
    fn upgrades_only_owned_bridge_and_can_resume_an_interrupted_upgrade() {
        let (dir, settings, display) = fixture();
        prepare(&settings, &display).unwrap();
        let target = root(&settings).unwrap();
        let marker = target.join(MARKER);
        let mut manifest: Value = serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        manifest["version"] = json!(1);
        for entry in manifest["files"].as_array_mut().unwrap() {
            if entry["name"] == "Scripts/DCSTabletControls.lua" {
                entry["after"] = json!("-- old owned bridge");
            }
        }
        write_json(&marker, &manifest).unwrap();
        fs::write(
            target.join("Scripts/DCSTabletControls.lua"),
            "-- old owned bridge",
        )
        .unwrap();
        prepare(&settings, &display).unwrap();
        assert_eq!(
            fs::read_to_string(target.join("Scripts/DCSTabletControls.lua")).unwrap(),
            BRIDGE
        );
        // A crash after the bridge write but before the manifest replacement is resumable.
        write_json(&marker, &manifest).unwrap();
        prepare(&settings, &display).unwrap();
        fs::write(target.join("Scripts/DCSTabletControls.lua"), "-- user edit").unwrap();
        assert!(prepare(&settings, &display).is_err());
        assert_eq!(
            fs::read_to_string(target.join("Scripts/DCSTabletControls.lua")).unwrap(),
            "-- user edit"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn malformed_manifest_cannot_delete_files() {
        let (dir, settings, display) = fixture();
        prepare(&settings, &display).unwrap();
        let target = root(&settings).unwrap();
        let marker = target.join(MARKER);
        let original = fs::read(&marker).unwrap();
        for bad in [
            json!({"version":3,"width":1,"height":1,"files":[]}),
            json!({"version":2,"width":1,"height":1,"files":[{"name":"../outside","after":""}]}),
        ] {
            write_json(&marker, &bad).unwrap();
            assert!(uninstall(&settings).is_err());
            assert!(target.join("Scripts/Export.lua").exists());
        }
        fs::write(&marker, original).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn isolated_profile_repeats_and_removes_without_touching_source() {
        let (dir, settings, display) = fixture();
        let original = fs::read(dir.join("DCS/Config/options.lua")).unwrap();
        prepare(&settings, &display).unwrap();
        prepare(&settings, &display).unwrap();
        let target = root(&settings).unwrap();
        let options = fs::read_to_string(target.join("Config/options.lua")).unwrap();
        assert!(options.contains("options.graphics.width = 2944"));
        assert!(options.contains("options.VR.enable = false"));
        assert!(
            target
                .join("Config/Input/FA-18C_hornet/joystick/test.diff.lua")
                .is_file()
        );
        uninstall(&settings).unwrap();
        assert!(!target.join("Scripts/DCSTabletControls.lua").exists());
        assert_eq!(
            original,
            fs::read(dir.join("DCS/Config/options.lua")).unwrap()
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn removal_archives_user_edits_missions_and_credentials_without_deleting() {
        let (dir, settings, display) = fixture();
        prepare(&settings, &display).unwrap();
        let target = root(&settings).unwrap();
        fs::write(target.join("Scripts/Export.lua"), "-- user exporter\n").unwrap();
        fs::create_dir_all(target.join("Missions")).unwrap();
        fs::write(target.join("Missions/my.miz"), b"sample mission\0data").unwrap();
        fs::write(
            target.join("Config/network.vault"),
            b"fake credentials fixture",
        )
        .unwrap();
        assert!(prepare(&settings, &display).is_err());
        let before = snapshot(&target);
        let archive = uninstall(&settings).unwrap();
        assert!(!target.exists());
        assert_eq!(snapshot(&archive), before);
        assert!(archive.parent() == target.parent());
        fs::remove_dir_all(dir).unwrap();
    }
    fn snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                assert!(filesystem::reject_link(&path).is_ok());
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.insert(
                        path.strip_prefix(root).unwrap().to_owned(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut result = std::collections::BTreeMap::new();
        walk(root, root, &mut result);
        result
    }
    fn junction(link: &Path, destination: &Path) {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(destination.to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn linked_input_cannot_delete_or_modify_the_source_profile() {
        let (dir, settings, display) = fixture();
        prepare(&settings, &display).unwrap();
        let target = root(&settings).unwrap();
        let link = target.join("Config/Input");
        fs::remove_dir_all(&link).unwrap();
        junction(&link, &dir.join("DCS/Config/Input"));
        let before = snapshot(&dir.join("DCS"));
        assert!(prepare(&settings, &display).is_err());
        assert!(uninstall(&settings).is_err());
        assert_eq!(snapshot(&dir.join("DCS")), before);
        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn linked_profile_root_is_never_claimed() {
        let (dir, settings, display) = fixture();
        let target = root(&settings).unwrap();
        junction(&target, &dir.join("DCS"));
        let before = snapshot(&dir.join("DCS"));
        assert!(prepare(&settings, &display).is_err());
        assert!(uninstall(&settings).is_err());
        assert_eq!(snapshot(&dir.join("DCS")), before);
        fs::remove_dir(target).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn partial_setup_can_be_archived_without_touching_source() {
        let (dir, settings, display) = fixture();
        prepare(&settings, &display).unwrap();
        let target = root(&settings).unwrap();
        fs::remove_file(target.join("Scripts/Export.lua")).unwrap();
        let before = snapshot(&target);
        let source = snapshot(&dir.join("DCS"));
        let archive = uninstall(&settings).unwrap();
        assert_eq!(snapshot(&archive), before);
        prepare(&settings, &display).unwrap();
        assert_eq!(snapshot(&dir.join("DCS")), source);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn atomic_writes_preserve_unrelated_temp_files_and_reject_shared_destinations() {
        let (dir, _, _) = fixture();
        let source = dir.join("DCS/Config/options.lua");
        let before = fs::read(&source).unwrap();
        let legacy = dir.join("state.tmp");
        fs::hard_link(&source, &legacy).unwrap();
        let state = dir.join("state.json");
        write_json(&state, &json!({"ok":true})).unwrap();
        assert_eq!(fs::read(&source).unwrap(), before);
        assert_eq!(fs::read(&legacy).unwrap(), before);
        fs::remove_file(&state).unwrap();
        fs::hard_link(&source, &state).unwrap();
        assert!(write_json(&state, &json!({"overwrite":false})).is_err());
        assert_eq!(fs::read(&source).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn nonreplacing_moves_never_clobber_existing_data() {
        let (dir, _, _) = fixture();
        let source = dir.join("move-source");
        let destination = dir.join("existing-backup");
        fs::write(&source, b"source").unwrap();
        fs::write(&destination, b"user backup").unwrap();
        assert!(filesystem::move_new(&source, &destination).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert_eq!(fs::read(&destination).unwrap(), b"user backup");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unowned_profile_is_never_overwritten() {
        let (dir, settings, display) = fixture();
        let target = root(&settings).unwrap();
        fs::create_dir_all(&target).unwrap();
        assert!(prepare(&settings, &display).is_err());
        assert!(!target.join(MARKER).exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn json_replacement_remains_readable() {
        let (dir, _, _) = fixture();
        let path = dir.join("state.json");
        write_json(&path, &json!({"revision":1})).unwrap();
        write_json(&path, &json!({"revision":2})).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(v["revision"], 2);
        fs::remove_dir_all(dir).unwrap();
    }
}
