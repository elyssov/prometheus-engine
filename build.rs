use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Human-readable HH:MM:SS of build (UTC, approximately)
    let hh = (secs / 3600) % 24;
    let mm = (secs / 60) % 60;
    let ss = secs % 60;
    let tag = format!("{:02}{:02}{:02}", hh, mm, ss);
    println!("cargo:rustc-env=BUILD_TAG={}", tag);
    println!("cargo:rustc-env=BUILD_EPOCH={}", secs);
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
}
