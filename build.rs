mod build_support;
use std::{
    env,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
fn utc_date(seconds: u64) -> String {
    // Gregorian date from Unix days, independent of the build machine's timezone.
    let z = (seconds / 86400) as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}
fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    for file in build_support::source_files(&root).expect("Read source files") {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    let features: Vec<_> = env::vars()
        .filter(|(key, _)| key.starts_with("CARGO_FEATURE_"))
        .collect();
    let hash = build_support::fingerprint(&root, &features).expect("Fingerprint source");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiler = Command::new(rustc)
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "unknown".into());
    println!("cargo:rustc-env=DCS_TABLET_BUILD_ID={hash}");
    println!("cargo:rustc-env=DCS_TABLET_COMPILER={compiler}");
    println!(
        "cargo:rustc-env=DCS_TABLET_TARGET={}",
        env::var("TARGET").unwrap()
    );
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let seconds = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .map(|s| {
            s.parse::<u64>()
                .expect("SOURCE_DATE_EPOCH must be Unix seconds")
        })
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });
    println!(
        "cargo:rustc-env=DCS_TABLET_BUILD_UNIX_MS={}",
        seconds * 1000
    );
    println!(
        "cargo:rustc-env=DCS_TABLET_BUILD_DATE={}",
        utc_date(seconds)
    );
}
