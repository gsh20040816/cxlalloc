use std::env;
use std::path::PathBuf;
use std::sync::LazyLock;

static OUT: LazyLock<PathBuf> = LazyLock::new(|| env::var("OUT_DIR").map(PathBuf::from).unwrap());

fn main() {
    cxlmalloc();
    boost();
}

fn cxlmalloc() {
    let cxlmalloc = pkg_config::probe_library("cxlmalloc").unwrap();
    println!("cargo:rustc-link-lib=atomic");
    bindgen::Builder::default()
        .header(
            cxlmalloc.include_paths[0]
                .join("cxlmalloc.h")
                .to_str()
                .unwrap(),
        )
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_args(["-x", "c++"])
        .allowlist_item("cxl_shm")
        .generate()
        .unwrap()
        .write_to_file(OUT.join("bind_cxl_shm.rs"))
        .unwrap();
}

fn boost() {
    let path = PathBuf::from("src/cpp").canonicalize().unwrap();

    let header = path.join("boost.hpp");
    let source = path.join("boost.cpp");
    let object = path.join("boost.o");
    let archive = path.join("libboost.a");

    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rustc-link-search={}", path.display());
    println!("cargo:rustc-link-lib=boost");
    println!("cargo:rustc-link-lib=dylib=stdc++");

    std::process::Command::new("clang++")
        .arg("-c")
        .arg("-o")
        .arg(&object)
        .arg(&source)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    std::process::Command::new("ar")
        .arg("rcs")
        .arg(&archive)
        .arg(&object)
        .output()
        .unwrap();

    bindgen::Builder::default()
        .header(header.to_str().unwrap())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_args(["-x", "c++"])
        .allowlist_item("wrap.*")
        .opaque_type("difference_type")
        .opaque_type("std.*")
        .opaque_type("boost.*")
        .generate()
        .unwrap()
        .write_to_file(OUT.join("bind_boost.rs"))
        .unwrap();
}
