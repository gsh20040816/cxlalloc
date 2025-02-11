use clap::Parser;

use crate::Allocator;
use crate::Backend;
use crate::benchmark;

#[derive(Clone, Parser)]
pub struct Ycsb {}

#[derive(Debug)]
pub struct Insert {
    key: &'static str,
    record: Vec<Field>,
}

#[derive(Debug)]
struct Field {
    name: &'static str,
    value: &'static str,
}

impl<B: Backend> benchmark::Interface<B> for Ycsb {
    type Global = Vec<Insert>;
    type Local = ();

    fn setup_process(&self, _: usize, _: usize, _: usize) -> Self::Global {
        let data = include_str!("../../ycsb-a.txt");

        let mut commands = Vec::new();
        for line in data.split('\n') {
            let Some(line) = line.strip_prefix("INSERT ") else {
                continue;
            };

            let (_table, line) = line.split_once(' ').unwrap();
            let (key, mut data) = line.split_once(' ').unwrap();
            data = data.strip_prefix('[').unwrap();

            let mut record = Vec::new();
            for _ in 0..10 {
                data = data.strip_prefix(' ').unwrap();
                let name = &data[..6];
                let value = &data[7..107];
                data = &data[107..];
                record.push(Field { name, value });
            }

            commands.push(Insert { key, record })
        }

        commands
    }

    fn setup_thread(&self, global: &Self::Global, thread_id: usize) -> Self::Local {
        todo!()
    }

    fn run_thread(
        &self,
        global: &Self::Global,
        local: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) {
        todo!()
    }
}

struct FlatMap<A: Allocator>([Option<A::Ptr>; 1 << 16]);
