use cxlalloc::Allocator;
use cxlalloc::Raw;

fn with_allocator<F: FnOnce(&mut Allocator)>(apply: F) {
    let _ = env_logger::try_init();
    let raw = Raw::builder().size_small(1 << 34).build("").unwrap();
    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let mut allocator = raw.allocator(id);
    apply(&mut allocator)
}

#[test]
fn smoke() {
    #[derive(Default)]
    struct Smoke {
        value: Option<cxlalloc::Box<u64>>,
    }

    with_allocator(|allocator| {
        let root = allocator.allocate::<Smoke>(None);

        assert_eq!(root.value.as_deref(), None);

        let value = allocator.allocate(Some(&mut root.value));

        *value = 16;

        assert_eq!(root.value.as_deref().copied(), Some(16));

        allocator.free(Some(&mut root.value));

        assert_eq!(root.value.as_deref(), None);

        allocator.free::<Smoke>(None);
    });
}
