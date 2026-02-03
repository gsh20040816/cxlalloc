use core::fmt::Display;

#[cfg(feature = "allocator-boost")]
pub mod boost;
#[cfg(feature = "allocator-cxl-shm")]
pub mod cxl_shm;
#[cfg(feature = "allocator-cxlalloc")]
pub mod cxlalloc;
#[cfg(feature = "allocator-lightning")]
pub mod lightning;
#[cfg(feature = "allocator-mimalloc")]
pub mod mimalloc;
#[cfg(feature = "allocator-ralloc")]
pub mod ralloc;

use cartesian::cartesian;
use serde::Deserialize;
use serde::Serialize;
use serde_inline_default::serde_inline_default;
use shm_bench::allocator::Coherence;
use shm_bench::allocator::Consistency;

use crate::TomlOption;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum Allocator {
    #[cfg(feature = "allocator-boost")]
    Boost,
    #[cfg(feature = "allocator-cxlalloc")]
    Cxlalloc(CxlallocCartesian),
    #[cfg(feature = "allocator-cxl-shm")]
    CxlShm,
    #[cfg(feature = "allocator-lightning")]
    Lightning,
    #[cfg(feature = "allocator-mimalloc")]
    Mimalloc(MimallocCartesian),
    #[cfg(feature = "allocator-ralloc")]
    Ralloc,
}

impl Allocator {
    #[cfg(feature = "allocator-cxlalloc")]
    pub fn cxlalloc() -> Self {
        Self::Cxlalloc(CxlallocCartesian::default())
    }

    #[cfg(feature = "allocator-mimalloc")]
    pub fn mimalloc() -> Self {
        Self::Mimalloc(MimallocCartesian::default())
    }
}

#[cfg(feature = "allocator-cxlalloc")]
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

#[cfg(feature = "allocator-cxlalloc")]
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

#[cfg(feature = "allocator-mimalloc")]
#[serde_inline_default]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MimallocCartesian {
    #[serde_inline_default(vec![true])]
    shm: Vec<bool>,
}

#[cfg(feature = "allocator-mimalloc")]
impl Default for MimallocCartesian {
    fn default() -> Self {
        // HACK: is there a better way to deduplicate...
        Self {
            shm: __serde_inline_default_MimallocCartesian_0(),
        }
    }
}

impl Allocator {
    pub fn for_each_cartesian<F: FnMut(shm_bench::allocator::Config<serde_json::Value>)>(
        &self,
        partial: Partial,
        mut apply: F,
    ) {
        let partial = partial.name(self.to_string());

        #[allow(unused_variables)]
        let config: Box<dyn Iterator<Item = _>> = match self {
            #[cfg(feature = "allocator-boost")]
            Allocator::Boost => Box::new(core::iter::once(serde_json::Value::Null)),
            #[cfg(feature = "allocator-cxl-shm")]
            Allocator::CxlShm => Box::new(core::iter::once(serde_json::Value::Null)),
            #[cfg(feature = "allocator-lightning")]
            Allocator::Lightning => Box::new(core::iter::once(serde_json::Value::Null)),
            #[cfg(feature = "allocator-mimalloc")]
            Allocator::Mimalloc(MimallocCartesian { shm }) => Box::new(
                shm.iter()
                    .map(|shm| mimalloc::Config::builder().shm(*shm).build())
                    .map(|config| serde_json::to_value(&config))
                    .map(Result::unwrap),
            ),
            #[cfg(feature = "allocator-ralloc")]
            Allocator::Ralloc => Box::new(core::iter::once(serde_json::Value::Null)),
            #[cfg(feature = "allocator-cxlalloc")]
            Allocator::Cxlalloc(CxlallocCartesian {
                cache_local,
                batch_bump,
                batch_global,
            }) => Box::new(
                cache_local
                    .iter()
                    .flat_map(move |cache_local| {
                        batch_bump.iter().flat_map(move |batch_bump| {
                            batch_global
                                .iter()
                                .map(move |batch_global| (cache_local, batch_bump, batch_global))
                        })
                    })
                    .map(|(cache_local, batch_bump, batch_global)| {
                        cxlalloc::Config::builder()
                            .cache_local(*cache_local)
                            .batch_bump(*batch_bump)
                            .batch_global(*batch_global)
                            .build()
                    })
                    .map(|config| serde_json::to_value(&config))
                    .map(Result::unwrap),
            ),
        };

        config.for_each(|config| apply(partial.clone().inner(config).build()))
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
} else if cfg!(feature = "consistency-sfence") as u64
    + cfg!(feature = "consistency-clflush") as u64
    + cfg!(feature = "consistency-clflushopt") as u64
    > 1
{
    panic!("Only one consistency flag can be set")
} else {
    Consistency::None
};

const COHERENCE: Coherence = if cfg!(feature = "cxl-limited") {
    Coherence::Limited
} else if cfg!(feature = "cxl-mcas") {
    Coherence::Mcas
} else if cfg!(feature = "cxl-limited") as u64 + cfg!(feature = "cxl-mcas") as u64 > 1 {
    panic!("Only one of cxl flag can be set")
} else {
    Coherence::None
};

type Partial = shm_bench::allocator::ConfigBuilder<
    serde_json::Value,
    shm_bench::allocator::config::SetCoherence<
        shm_bench::allocator::config::SetConsistency<
            shm_bench::allocator::config::SetPopulate<
                shm_bench::allocator::config::SetSize<shm_bench::allocator::config::SetNuma>,
            >,
        >,
    >,
>;

impl Config {
    pub fn for_each_cartesian<F: FnMut(Partial)>(&self, mut apply: F) {
        cartesian!(&self.numa, &self.size, &self.populate).for_each(|(numa, size, populate)| {
            let config = shm_bench::allocator::Config::builder()
                .maybe_numa(numa.0.clone())
                .size(*size)
                .maybe_populate(populate.0)
                .consistency(CONSISTENCY)
                .coherence(COHERENCE);

            apply(config)
        })
    }
}

impl Display for Allocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            #[cfg(feature = "allocator-boost")]
            Allocator::Boost => "boost",
            #[cfg(feature = "allocator-cxlalloc")]
            Allocator::Cxlalloc { .. } => "cxlalloc",
            #[cfg(feature = "allocator-cxl-shm")]
            Allocator::CxlShm => "cxl_shm",
            #[cfg(feature = "allocator-lightning")]
            Allocator::Lightning => "lightning",
            #[cfg(feature = "allocator-mimalloc")]
            Allocator::Mimalloc(_) => "mimalloc",
            #[cfg(feature = "allocator-ralloc")]
            Allocator::Ralloc => "ralloc",
        };

        write!(f, "{name}")
    }
}
