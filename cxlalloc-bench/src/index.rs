use core::fmt::Display;

use cartesian::cartesian;
use clap::ValueEnum;
use serde::Deserialize;
use serde::Serialize;
use serde_inline_default::serde_inline_default;

use crate::TomlOption;

#[derive(Copy, Clone, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum Index {
    Linear,
    Linked,
}

#[serde_inline_default]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde_inline_default(vec![1 << 25])]
    len: Vec<usize>,

    #[serde_inline_default(vec![TomlOption(None)])]
    populate: Vec<TomlOption<shm::Populate>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            len: __serde_inline_default_Config_0(),
            populate: __serde_inline_default_Config_1(),
        }
    }
}

impl Config {
    pub fn for_each_cartesian<F: FnMut(shm_bench::index::Config)>(
        &self,
        index: Index,
        mut apply: F,
    ) {
        cartesian!(&self.len, &self.populate).for_each(|(len, populate)| {
            let config = shm_bench::index::Config::builder()
                .name(index.to_string())
                .len(*len)
                .maybe_populate(populate.0)
                .build();

            apply(config)
        })
    }
}

impl Display for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Linked => "linked",
            Self::Linear => "linear",
        };
        write!(f, "{name}")
    }
}
