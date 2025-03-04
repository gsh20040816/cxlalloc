use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

#[derive(Copy, Clone, Serialize)]
pub enum Allocator {
    Mi2,
    Je,
    Cxl,
    CxlDebug,
    CxlMi2,
    CxlShm,
    R,
}

impl Allocator {
    pub fn path(&self) -> PathBuf {
        let path = match self {
            Allocator::Mi2 => "mi2/out/release/libmimalloc",
            Allocator::Je => "je/lib/libjemalloc",
            Allocator::Cxl => "cxlalloc/target/release/libcxlalloc_dynamic",
            Allocator::CxlMi2 => "cxl-mi2/build/libcxl_mimalloc_dynamic",
            Allocator::CxlDebug => "cxlalloc/target/debug/libcxlalloc_dynamic",
            Allocator::CxlShm => "cxl-shm/build/libcxlmalloc_dynamic",
            Allocator::R => "r/build/libralloc_dynamic",
        };

        // TODO: change for MacOS
        let ext = "so";
        let path = Path::new("extern").join(Path::new(path).with_extension(ext));

        path
    }
}
