//! Shared source fingerprint calculation for build stamping and release checks.
//! This identifies source/feature mismatches; it is not a cryptographic signature.
use std::{fs, path::Path};
pub fn source_files(root: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut files = fs::read_dir(root.join("src"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.retain(|path| path.extension().is_some_and(|extension| extension == "rs"));
    files.extend(
        [
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "build_support.rs",
            "web/index.html",
            "dcs/DCSTabletControls.lua",
        ]
        .map(|name| root.join(name)),
    );
    files.sort();
    Ok(files)
}
pub fn fingerprint(root: &Path, features: &[(String, String)]) -> std::io::Result<String> {
    let mut hash = 0xcbf29ce484222325u64;
    for file in source_files(root)? {
        for byte in fs::read(file)? {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    let mut features = features.to_vec();
    features.sort();
    for byte in format!("{features:?}").bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    Ok(format!("{hash:016x}"))
}
