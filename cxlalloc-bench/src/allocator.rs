use std::path::Path;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Context as _;
use clap::ValueEnum;

#[derive(Copy, Clone, ValueEnum)]
pub enum Allocator {
    Mi2,
    Je,
    CxlOld,
    CxlNew,
}

impl Allocator {
    pub fn path(&self, release: bool) -> anyhow::Result<PathBuf> {
        let path = match self {
            Allocator::Mi2 => "mi2/out/release/libmimalloc",
            Allocator::Je => "je/lib/libjemalloc",
            Allocator::CxlOld if release => "cxlalloc-old/target/release/libcxlalloc_dynamic",
            Allocator::CxlOld => "cxlalloc-old/target/debug/libcxlalloc_dynamic",
            Allocator::CxlNew if release => "cxlalloc-new/target/release/libcxlalloc_dynamic",
            Allocator::CxlNew => "cxlalloc-new/target/debug/libcxlalloc_dynamic",
        };

        // TODO: change for MacOS
        let ext = "so";
        let path = Path::new("extern").join(Path::new(path).with_extension(ext));

        path.canonicalize()
            .with_context(|| anyhow!("Could not find allocator at {}", path.display()))
    }
}
