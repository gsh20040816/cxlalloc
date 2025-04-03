use core::fmt::Display;

use clap::ValueEnum;
use serde::Deserialize;
use serde::Serialize;

#[derive(Copy, Clone, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Index {
    Linear,
    Linked,
}

impl Display for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Linked => "linked",
            Self::Linear => "linear",
        };
        write!(f, "{}", name)
    }
}
