// https://github.com/nwtnni/sosp-paper19-ae/blob/main/test/benchmark/kv.cpp

use crate::KeyValueStore;

pub struct Configuration {
    read_ratio: usize,
    operation_count: usize,
}

struct Generator;

impl Generator {
    fn next(&mut self) -> u64 {
        todo!();
    }
}

fn worker<K: KeyValueStore>(configuration: &Configuration, mut kvs: K) {
    let mut i = configuration.operation_count as isize;
    let id = kvs.thread_id() - 1;
    let len = 1 + configuration.read_ratio as isize;
    let mut rng = Generator;

    while i > 0 {
        let key = (id << 56) + rng.next();
        let value = rng.next();
        kvs.put(key, value);

        for _ in 0..configuration.read_ratio {
            let id = rng.next() & 63;
            let key = (id << 56) + rng.next();
            let _ = kvs.get(key);
        }

        i -= len;
    }

    if i == 0 {
        return;
    }

    for _ in 0..len - i {
        let id = rng.next() & 63;
        let key = (id << 56) + rng.next();
        let _ = kvs.get(key);
    }
}
