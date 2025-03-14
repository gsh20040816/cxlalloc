use crossbeam_channel::Receiver;
use crossbeam_channel::Sender;
use memento::ds::clevel::Clevel;
use memento::ds::clevel::Delete;
use memento::ds::clevel::Insert;
use memento::ds::clevel::Resize;
use memento::ploc::Checkpoint;
use memento::ploc::Handle;
use memento::pmem::Collectable;
use memento::pmem::GarbageCollection;
use memento::pmem::PoolHandle;
use memento::pmem::RootObj;
use memento::Collectable;
use memento::Memento;

pub static mut SEND: Option<[Option<Sender<()>>; 64]> = None;
pub static mut RECV: Option<Receiver<()>> = None;

#[derive(Default, Collectable, Memento)]
pub struct Mmt {
    resize: Resize<u64, u64>,

    i: Checkpoint<u64>,
    insert: Insert<u64, u64>,
    delete: Delete<u64, u64>,
}

impl RootObj<Mmt> for Clevel<u64, u64> {
    fn run(&self, mmt: &mut Mmt, handle: &Handle) {
        let tid = handle.tid;

        match tid {
            // T1: Resize loop
            1 => {
                let recv = unsafe { RECV.as_ref().unwrap() };
                self.resize(&recv, &mut mmt.resize, handle);
            }
            _ => {
                let mut i = 0;
                let send = unsafe { SEND.as_ref().unwrap()[tid].as_ref().unwrap() };

                while i < 1_000_000 {
                    i = mmt.i.checkpoint(|| i + 1, handle);

                    let key = (tid as u64) << 32 | i;
                    let value = key * 2;

                    assert!(self
                        .insert(key, value, send, &mut mmt.insert, handle)
                        .is_ok());
                    assert_eq!(self.search(&key, handle), Some(&value));

                    assert!(self.delete(&key, &mut mmt.delete, handle));
                    assert_eq!(self.search(&value, handle), None);
                }

                unsafe {
                    SEND.as_mut().unwrap()[tid].take();
                }
            }
        }
    }
}
