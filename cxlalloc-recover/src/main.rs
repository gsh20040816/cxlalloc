use memento::ds::queue::Dequeue;
use memento::ds::queue::Enqueue;
use memento::ds::queue::Queue;

use memento::ploc::Checkpoint;
use memento::ploc::Handle;
use memento::pmem::{Collectable, GarbageCollection, Pool, PoolHandle, RootObj};
use memento::{Collectable, Memento};

#[derive(Memento, Default, Collectable)]
struct Mmt {
    i: Checkpoint<u64>,
    enq: Enqueue<u64>,
    j: Checkpoint<u64>,
    deq: Dequeue<u64>,
}

impl RootObj<Mmt> for Queue<u64> {
    fn run(&self, mmt: &mut Mmt, handle: &Handle) {
        let mut i = 0;
        while i < 1000 {
            self.enqueue(i, &mut mmt.enq, handle);
            i = mmt.i.checkpoint(|| i + 1, handle);
        }

        let mut j = 0;
        while j < 1000 {
            assert_eq!(j, self.dequeue(&mut mmt.deq, handle).unwrap());
            j = mmt.j.checkpoint(|| j + 1, handle);
        }
    }
}

fn main() {
    let pool = Pool::create::<Queue<u64>, Mmt>("/dev/shm/pool", 1 << 32, 1).unwrap();

    pool.execute::<Queue<u64>, Mmt>();
}
