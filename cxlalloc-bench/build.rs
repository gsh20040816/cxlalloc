use std::env;
use std::path::PathBuf;
use std::sync::LazyLock;

static OUT: LazyLock<PathBuf> = LazyLock::new(|| env::var("OUT_DIR").map(PathBuf::from).unwrap());

fn main() {
    cxlmalloc();
    lightning();
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

fn lightning() {
    let lightning = pkg_config::probe_library("lightning").unwrap();
    bindgen::Builder::default()
        .header(
            lightning.include_paths[0]
                .join("allocator.h")
                .to_str()
                .unwrap(),
        )
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_args(["-x", "c++"])
        .allowlist_item("LightningAllocator")
        .opaque_type("std.*")
        .generate()
        .unwrap()
        .write_to_file(OUT.join("bind_lightning.rs"))
        .unwrap();
}

fn boost() {
    let path = PathBuf::from("src/cpp").canonicalize().unwrap();

    let header = path.join("boost.hpp");
    let source = path.join("boost.cpp");

    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", source.display());

    cxx_build::bridge("src/process/boost.rs")
        .file("src/cpp/boost.cpp")
        .compile("boost");
}
