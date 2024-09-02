use cxlalloc::raw;
use cxlalloc::root;
use cxlalloc::Allocator;
use cxlalloc::Root;

fn with_allocator<F: FnOnce(&mut Allocator)>(apply: F) {
    let _ = env_logger::try_init();
    let raw = raw::Builder::default().build("").unwrap();
    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let mut allocator = raw.allocator(id);
    apply(&mut allocator)
}

#[test]
fn create() {
    let raw = raw::Builder::default().build("").unwrap();
    let _heap = raw.heap();
    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let _allocator = raw.allocator(id);
}

#[test]
fn allocate_small() {
    with_allocator(|allocator| {
        let mut root: Root<u64> = unsafe { allocator.root(root::Index::new(0)) };
        let root = allocator.allocate_at(&mut root);
        assert_eq!(*root, 0);
    })
}
