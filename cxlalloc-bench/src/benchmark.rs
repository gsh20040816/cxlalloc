use std::path::Path;

use clap::ValueEnum;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Benchmark {
    Cfrac,
    Espresso,
    Barnes,
    Redis,
    Lean,
    LarsonSized,
    Mstress,
    Rptest,
    Gs,
    Lua,
    AllocTest,
    Sh6bench,
    Sh8bench,
    XmallocTest,
    Cscratch,
    GlibcSimple,
    GlibcThread,
    Rocksdb,
    Larson,
    LeanMathlib,
    Mleak,
    Rbstress,
    Cthrash,
    Z3,
}

impl Benchmark {
    pub fn path(&self) -> &'static Path {
        macro_rules! out {
            ($relative:expr) => {
                concat!("out/bench/", $relative)
            };
        }

        let path = match self {
            Benchmark::Cfrac => out!("cfrac"),
            Benchmark::Espresso => out!("espresso"),
            Benchmark::Barnes => out!("barnes"),
            Benchmark::Redis => todo!(),
            Benchmark::Lean => todo!(),
            Benchmark::LarsonSized => out!("larson-sized"),
            Benchmark::Mstress => out!("mstress"),
            Benchmark::Rptest => out!("rptest"),
            Benchmark::Gs => todo!(),
            Benchmark::Lua => todo!(),
            Benchmark::AllocTest => out!("alloc-test"),
            Benchmark::Sh6bench => out!("sh6bench"),
            Benchmark::Sh8bench => out!("sh8bench"),
            Benchmark::XmallocTest => out!("xmalloc-test"),
            Benchmark::Cscratch => out!("cache-scratch"),
            Benchmark::GlibcSimple => out!("glibc-simple"),
            Benchmark::GlibcThread => out!("glibc-thread"),
            Benchmark::Rocksdb => todo!(),
            Benchmark::Larson => out!("larson"),
            Benchmark::LeanMathlib => todo!(),
            Benchmark::Mleak => out!("mleak"),
            Benchmark::Rbstress => todo!(),
            Benchmark::Cthrash => out!("cache-thrash"),
            Benchmark::Z3 => todo!(),
        };

        Path::new(path)
    }

    pub fn args(&self) -> Vec<String> {
        let threads = std::thread::available_parallelism().unwrap().get();

        macro_rules! args {
            ($($args:expr),* $(,)?) => {
                vec![ $($args.to_string()),* ]
            }
        }

        match self {
            Benchmark::Cfrac => args!["17545186520507317056371138836327483792789528"],
            Benchmark::Espresso => args!["bench/espresso/largest.espresso"],
            Benchmark::Barnes => vec![],
            Benchmark::Redis => todo!(),
            Benchmark::Lean => todo!(),
            Benchmark::LarsonSized => args![5, 8, 1000, 5000, 100, 4141, threads],
            Benchmark::Mstress => args![threads, 50, 25],
            Benchmark::Rptest => args![threads, 0, 1, 2, 500, 1000, 100, 8, 16000],
            Benchmark::Gs => todo!(),
            Benchmark::Lua => todo!(),
            Benchmark::AllocTest => todo!(),
            Benchmark::Sh6bench => args![threads * 2],
            Benchmark::Sh8bench => args![threads * 2],
            Benchmark::XmallocTest => args!["-w", threads, "-t", 5, "-s", 64],
            Benchmark::Cscratch => todo!(),
            Benchmark::GlibcSimple => todo!(),
            Benchmark::GlibcThread => todo!(),
            Benchmark::Rocksdb => todo!(),
            Benchmark::Larson => args![5, 8, 1000, 5000, 100, 4141, threads],
            Benchmark::LeanMathlib => todo!(),
            Benchmark::Mleak => todo!(),
            Benchmark::Rbstress => todo!(),
            Benchmark::Cthrash => todo!(),
            Benchmark::Z3 => todo!(),
        }
    }

    pub fn command(&self) -> String {
        let mut command = self.path().display().to_string();
        for arg in self.args() {
            command.push(' ');
            command.push_str(&arg);
        }
        command
    }
}
