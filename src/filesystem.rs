//! Conservative filesystem boundaries for profile and persistent-state changes.
//! Existing links/shared files are refused before modification. This protects
//! ordinary linked-profile arrangements, not hostile concurrent filesystem edits.
use std::{
    fs::{self, Metadata, OpenOptions},
    os::windows::{fs::MetadataExt, io::AsRawHandle},
    path::{Component, Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle, MOVE_FILE_FLAGS, MoveFileExW,
        },
    },
    core::PCWSTR,
};

pub fn reject_link(path: &Path) -> Result<Option<Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_attributes() & 0x400 != 0 => Err(format!(
            "Linked/reparse path is not supported: {}. Keep an independent profile copy.",
            path.display()
        )),
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Cannot inspect {}: {error}", path.display())),
    }
}
pub fn reject_shared_file(path: &Path) -> Result<(), String> {
    let Some(metadata) = reject_link(path)? else {
        return Ok(());
    };
    if !metadata.is_file() {
        return Err(format!("Expected a regular file: {}", path.display()));
    }
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the borrowed Windows file handle remains open for this synchronous
    // query; info is initialized and no pointer escapes the call.
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
        .map_err(|e| e.to_string())?;
    if info.nNumberOfLinks > 1 {
        return Err(format!(
            "Shared hard-linked file is not supported: {}",
            path.display()
        ));
    }
    Ok(())
}
/// Inspect each component below an owned root without following directory links.
pub fn profile_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    reject_link(root)?;
    let mut path = root.to_owned();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("Unsafe profile-relative path".into());
        };
        path.push(name);
        reject_link(&path)?;
    }
    if path.is_file() {
        reject_shared_file(&path)?;
    }
    Ok(path)
}
/// Unlike a replacing rename, this never overwrites an existing destination.
pub fn move_new(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let source = wide(source);
    let destination = wide(destination);
    // SAFETY: both NUL-terminated buffers live through this synchronous call.
    // Flags zero deliberately disallow replacement of any existing destination.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVE_FILE_FLAGS(0),
        )
    }
    .map_err(|e| e.to_string())
}
