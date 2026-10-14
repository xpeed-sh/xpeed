use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/xpeed.rc");
    println!("cargo:rerun-if-changed=assets/img/xpeed.ico");
    println!("cargo:rerun-if-env-changed=WINDRES");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let resource = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR")).join(if msvc {
        "xpeed.res"
    } else {
        "xpeed-icon.o"
    });
    let status = if msvc {
        Command::new("rc")
            .args(["/nologo", "/fo"])
            .arg(&resource)
            .arg("assets/xpeed.rc")
            .status()
    } else {
        Command::new(env::var("WINDRES").unwrap_or_else(|_| "windres".into()))
            .args(["-i", "assets/xpeed.rc", "-O", "coff", "-o"])
            .arg(&resource)
            .status()
    }
    .expect("Windows icon requires rc.exe (MSVC) or windres (GNU) in PATH");
    assert!(status.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg-bin=xpeed={}", resource.display());
}
