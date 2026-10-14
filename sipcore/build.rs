//! Builds csrc/shim.c and links PJSIP.
//!
//! Set PJSIP_DIR before building:
//! - macOS / Linux: the `--prefix` you passed to pjproject's `make install`
//!   (the folder that contains `lib/pkgconfig/libpjproject.pc`).
//! - Windows: the pjproject source folder after building
//!   the `libpjproject` target of `pjproject-vs14.sln` as `Release-Dynamic|x64`.
//!
//! The scripts in `scripts/` do both builds for you.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=csrc/shim.c");
    println!("cargo:rerun-if-changed=csrc/shim.h");
    println!("cargo:rerun-if-env-changed=PJSIP_DIR");

    let pj_dir = PathBuf::from(env::var("PJSIP_DIR").unwrap_or_else(|_| {
        panic!("PJSIP_DIR is not set. Run scripts/build-pjsip-* first, then set PJSIP_DIR (see README).")
    }));

    let mut build = cc::Build::new();
    build.file("csrc/shim.c").include("csrc").warnings(false);

    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        link_windows(&pj_dir, &mut build);
    } else {
        link_pkg_config(&pj_dir, &mut build);
    }

    build.compile("sipshim");
}

fn link_pkg_config(pj_dir: &Path, build: &mut cc::Build) {
    let pc_dir = pj_dir.join("lib").join("pkgconfig");
    let old = env::var("PKG_CONFIG_PATH").unwrap_or_default();
    let sep = if old.is_empty() { "" } else { ":" };
    env::set_var("PKG_CONFIG_PATH", format!("{}{}{}", pc_dir.display(), sep, old));

    let lib = pkg_config::Config::new()
        .statik(true)
        .cargo_metadata(false)
        .probe("libpjproject")
        .expect("libpjproject not found; check PJSIP_DIR");

    // Emit the link flags ourselves so we can leave out pjsua2 (the C++ API)
    // and the C++ runtime it drags in; we only use the C API.
    for dir in &lib.link_paths {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    for name in &lib.libs {
        if name.starts_with("pjsua2") || name == "stdc++" || name == "c++" {
            continue;
        }
        let is_static = lib
            .link_paths
            .iter()
            .any(|d| d.join(format!("lib{name}.a")).exists());
        let kind = if is_static { "static" } else { "dylib" };
        println!("cargo:rustc-link-lib={kind}={name}");
    }
    for dir in &lib.framework_paths {
        println!("cargo:rustc-link-search=framework={}", dir.display());
    }
    for fw in &lib.frameworks {
        println!("cargo:rustc-link-lib=framework={fw}");
    }
    for inc in &lib.include_paths {
        build.include(inc);
    }
    for (k, v) in &lib.defines {
        build.define(k, v.as_deref());
    }
}

fn link_windows(pj_dir: &Path, build: &mut cc::Build) {
    for sub in ["pjlib", "pjlib-util", "pjnath", "pjmedia", "pjsip"] {
        build.include(pj_dir.join(sub).join("include"));
    }
    build.define("PJ_WIN32", "1").define("PJ_M_X86_64", "1");

    let config = env::var("PJSIP_CONFIG").unwrap_or_else(|_| "Release-Dynamic".into());
    println!("cargo:rerun-if-env-changed=PJSIP_CONFIG");
    let suffix = format!("-{}.lib", config);

    // Building the `libpjproject` target produces one combined library in
    // lib/. Fall back to the per-module libraries if that's not there.
    let wanted = |name: &str| name.ends_with(&suffix) && name.contains("x64") && !name.starts_with("pjsua2");
    let combined_dir = pj_dir.join("lib");
    let combined: Vec<String> = fs::read_dir(&combined_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("libpjproject") && wanted(n))
        .collect();

    let mut found = 0;
    if let Some(name) = combined.first() {
        println!("cargo:rustc-link-search=native={}", combined_dir.display());
        println!("cargo:rustc-link-lib=static={}", name.trim_end_matches(".lib"));
        found = 1;
    } else {
        for sub in ["pjlib", "pjlib-util", "pjnath", "pjmedia", "pjsip", "third_party"] {
            let dir = pj_dir.join(sub).join("lib");
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            println!("cargo:rustc-link-search=native={}", dir.display());
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if wanted(&name) {
                    println!("cargo:rustc-link-lib=static={}", name.trim_end_matches(".lib"));
                    found += 1;
                }
            }
        }
    }
    if found == 0 {
        panic!(
            "No PJSIP libraries ending in {suffix} under {}. Build the libpjproject target of pjproject-vs14.sln as {config}|x64.",
            pj_dir.display()
        );
    }
    for sys in [
        "ws2_32", "iphlpapi", "ole32", "oleaut32", "winmm", "user32", "advapi32",
        "gdi32", "crypt32", "secur32", "ncrypt", "bcrypt", "dsound", "dxguid", "strmiids",
        "mfplat", "mfuuid", "ksuser", "uuid",
    ] {
        println!("cargo:rustc-link-lib=dylib={sys}");
    }
}
