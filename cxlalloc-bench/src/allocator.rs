use core::fmt::Display;

pub mod boost;
pub mod cxl_shm;
pub mod cxlalloc;
pub mod lightning;
pub mod mimalloc;
pub mod ralloc;

pub use boost::Boost;
pub use cxl_shm::CxlShm;
pub use cxlalloc::Cxlalloc;
pub use lightning::Lightning;
pub use mimalloc::Mimalloc;
pub use ralloc::Ralloc;

use allocator_bench::allocator::Consistency;
use cartesian::cartesian;
use serde::Deserialize;
use serde::Serialize;
use serde_inline_default::serde_inline_default;

use crate::TomlOption;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum Allocator {
    Boost,
    Cxlalloc(CxlallocCartesian),
    CxlShm,
    Lightning,
    Mimalloc,
    Ralloc,
}

impl Allocator {
    pub fn cxlalloc() -> Self {
        Self::Cxlalloc(CxlallocCartesian::default())
    }
}

#[serde_inline_default]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CxlallocCartesian {
    #[serde_inline_default(vec![1])]
    cache_local: Vec<usize>,

    #[serde_inline_default(vec![1])]
    batch_bump: Vec<usize>,

    #[serde_inline_default(vec![1])]
    batch_global: Vec<usize>,
}

impl Default for CxlallocCartesian {
    fn default() -> Self {
        // HACK: is there a better way to deduplicate...
        Self {
            cache_local: __serde_inline_default_CxlallocCartesian_0(),
            batch_bump: __serde_inline_default_CxlallocCartesian_1(),
            batch_global: __serde_inline_default_CxlallocCartesian_2(),
        }
    }
}

impl Allocator {
    pub fn for_each_cartesian<F: FnMut(allocator_bench::allocator::Config<serde_json::Value>)>(
        &self,
        partial: Partial,
        mut apply: F,
    ) {
        let partial = partial.name(self.to_string());
        match self {
            Allocator::Boost
            | Allocator::CxlShm
            | Allocator::Lightning
            | Allocator::Mimalloc
            | Allocator::Ralloc => apply(partial.inner(serde_json::Value::Null).build()),
            Allocator::Cxlalloc(CxlallocCartesian {
                cache_local,
                batch_bump,
                batch_global,
            }) => cartesian!(&cache_local, &batch_bump, &batch_global).for_each(
                |(cache_local, batch_bump, batch_global)| {
                    let config = cxlalloc::Config::builder()
                        .cache_local(*cache_local)
                        .batch_bump(*batch_bump)
                        .batch_global(*batch_global)
                        .build();

                    apply(
                        partial
                            .clone()
                            .inner(serde_json::to_value(&config).unwrap())
                            .build(),
                    )
                },
            ),
        }
    }
}

#[serde_inline_default]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde_inline_default(vec![TomlOption(None)])]
    numa: Vec<TomlOption<shm::Numa>>,

    // 2^36 = 64 GiB
    #[serde_inline_default(vec![1 << 36])]
    size: Vec<usize>,

    #[serde_inline_default(vec![TomlOption(None)])]
    populate: Vec<TomlOption<shm::Populate>>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            numa: __serde_inline_default_Config_0(),
            size: __serde_inline_default_Config_1(),
            populate: __serde_inline_default_Config_2(),
        }
    }
}

const CONSISTENCY: Consistency = if cfg!(feature = "consistency-sfence") {
    Consistency::Sfence
} else if cfg!(feature = "consistency-clflush") {
    Consistency::Clflush
} else if cfg!(feature = "consistency-clflushopt") {
    Consistency::Clflushopt
} else {
    Consistency::None
};

type Partial = allocator_bench::allocator::ConfigBuilder<
    serde_json::Value,
    allocator_bench::allocator::config::SetConsistency<
        allocator_bench::allocator::config::SetPopulate<
            allocator_bench::allocator::config::SetSize<
                allocator_bench::allocator::config::SetNuma,
            >,
        >,
    >,
>;

impl Config {
    pub fn for_each_cartesian<F: FnMut(Partial)>(&self, mut apply: F) {
        cartesian!(&self.numa, &self.size, &self.populate).for_each(|(numa, size, populate)| {
            let config = allocator_bench::allocator::Config::builder()
                .maybe_numa(numa.0.clone())
                .size(*size)
                .maybe_populate(populate.0)
                .consistency(CONSISTENCY);

            apply(config)
        })
    }
}

impl Display for Allocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Allocator::Boost => "boost",
            Allocator::Cxlalloc { .. } => "cxlalloc",
            Allocator::CxlShm => "cxl_shm",
            Allocator::Lightning => "lightning",
            Allocator::Mimalloc => "mimalloc",
            Allocator::Ralloc => "ralloc",
        };

        write!(f, "{}", name)
    }
}
