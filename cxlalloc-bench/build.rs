use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::sync::LazyLock;

static OUT: LazyLock<PathBuf> = LazyLock::new(|| env::var("OUT_DIR").map(PathBuf::from).unwrap());

const CONSISTENCY: Option<&str> = if cfg!(feature = "consistency-sfence") {
    Some("CONSISTENCY_SFENCE")
} else if cfg!(feature = "consistency-clflush") {
    Some("CONSISTENCY_CLFLUSH")
} else if cfg!(feature = "consistency-clflushopt") {
    Some("CONSISTENCY_CLFLUSHOPT")
} else {
    None
};

fn main() {
    cxlmalloc();
    lightning();
    boost();
    mimalloc();
}

fn cxlmalloc() {
    let path = Path::new("../extern/sosp-paper19-ae")
        .canonicalize()
        .unwrap();

    let mut config = cmake::Config::new(&path);

    if let Some(consistency) = CONSISTENCY {
        config.cxxflag(format!("-D{}", consistency));
    }

    let root = config
        .out_dir(OUT.join("cxlmalloc"))
        .build_target("cxlmalloc")
        .build();

    println!("cargo:rustc-link-search=native={}/build", root.display());
    println!("cargo:rustc-link-lib=static=cxlmalloc");

    // NOTE: rustc-link-lib=static=atomic does *not* work,
    // presumably because the underlying linker knows where
    // to find libatomic even if rustc doesn't. Not sure if this
    // is a NixOS thing, or because libatomic is bundled with
    // the GCC toolchain (?)
    println!("cargo:rustc-link-arg=-latomic");

    bindgen::Builder::default()
        .header(path.join("include/cxlmalloc.h").to_str().unwrap())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_args(["-x", "c++"])
        .allowlist_item("cxl_shm")
        .generate()
        .unwrap()
        .write_to_file(OUT.join("bind_cxl_shm.rs"))
        .unwrap();
}

fn lightning() {
    let path = Path::new("../extern/lightning").canonicalize().unwrap();

    let root = cmake::Config::new(&path)
        .out_dir(OUT.join("lightning"))
        .build_target("lightning")
        .build();

    println!("cargo:rustc-link-search=native={}/build", root.display());
    println!("cargo:rustc-link-lib=static=lightning");

    bindgen::Builder::default()
        .header(path.join("inc").join("allocator.h").to_str().unwrap())
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

    cxx_build::bridge("src/allocator/boost.rs")
        .file("src/cpp/boost.cpp")
        .opt_level(3)
        .compile("boost");
}

fn mimalloc() {
    let path = Path::new("../extern/mimalloc").canonicalize().unwrap();

    let mut config = cmake::Config::new(&path);

    let root = config
        .out_dir(OUT.join("mimalloc"))
        .define("MI_BUILD_SHARED", "OFF")
        .define("MI_BUILD_OBJECT", "OFF")
        .define("MI_BUILD_TESTS", "OFF")
        .build_target("mimalloc-static")
        .build();

    println!("cargo:rustc-link-search=native={}/build", root.display());
    println!(
        "cargo:rustc-link-lib=static=mimalloc{}",
        if cfg!(debug_assertions) { "-debug" } else { "" },
    );

    bindgen::Builder::default()
        .header(path.join("include").join("mimalloc.h").to_str().unwrap())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .unwrap()
        .write_to_file(OUT.join("bind_mimalloc.rs"))
        .unwrap();
}
