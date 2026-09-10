use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=resources/app.rc");
    println!("cargo:rerun-if-changed=resources/app.manifest");
    println!("cargo:rerun-if-changed=assets/krun.ico");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let resource = out_dir.join("krun.res");
    let compiler = find_resource_compiler().expect("Windows SDK rc.exe was not found");
    let status = Command::new(compiler)
        .args(["/nologo", "/fo"])
        .arg(&resource)
        .arg("resources/app.rc")
        .status()
        .expect("failed to run rc.exe");
    assert!(status.success(), "rc.exe failed to compile KRun resources");
    println!("cargo:rustc-link-arg={}", resource.display());
}

fn find_resource_compiler() -> Option<PathBuf> {
    if let Some(sdk) = env::var_os("WindowsSdkDir") {
        let root = PathBuf::from(sdk).join("bin");
        if let Some(path) = newest_rc(&root) {
            return Some(path);
        }
    }
    let roots = [
        PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin"),
        PathBuf::from(r"C:\Program Files\Windows Kits\10\bin"),
    ];
    roots.iter().find_map(|root| newest_rc(root))
}

fn newest_rc(root: &Path) -> Option<PathBuf> {
    let direct = root.join("x64").join("rc.exe");
    if direct.exists() {
        return Some(direct);
    }
    let mut versions = fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    versions.sort();
    versions.reverse();
    versions
        .into_iter()
        .map(|version| version.join("x64").join("rc.exe"))
        .find(|path| path.exists())
}
