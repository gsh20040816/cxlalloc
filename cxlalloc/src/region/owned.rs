use core::alloc::Layout;

use crate::raw;
use crate::size;
use crate::slab;
use crate::thread;
use crate::SIZE_PAGE;

pub(crate) struct Owned<'raw> {
    pub(crate) meta: thread::Slice<'raw, Meta>,
    pub(crate) slabs: slab::Slice<'raw, slab::Owned>,
}

impl<'raw> Owned<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::new::<thread::Array<Meta>>()
            .extend(slab::Slice::<slab::Owned>::layout(slab_count).unwrap())
            .unwrap()
            .0
            .align_to(SIZE_PAGE)
            .unwrap()
            .pad_to_align()
    }

    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner) -> Self {
        // FIXME: deduplicate with `layout`
        let (_, offset) = Layout::new::<thread::Array<Meta>>()
            .extend(slab::Slice::<slab::Owned>::layout(1).unwrap())
            .unwrap();

        Self {
            meta: thread::Slice::from_raw(&raw.owned, 0),
            slabs: slab::Slice::from_raw(&raw.owned, offset),
        }
    }
}

#[repr(C, align(64))]
pub(crate) struct Meta {
    pub(crate) state: Option<()>,
    pub(crate) r#unsized: slab::LocalStack,
    pub(crate) r#sized: size::Array<slab::LocalStack>,
}

impl Meta {
    pub(crate) fn unsized_to_sized(
        &mut self,
        owned: &slab::Slice<slab::Owned>,
        shared: &slab::Slice<slab::Shared>,
        class: size::Small,
    ) -> bool {
        let Some(index) = self.r#unsized.peek() else {
            return false;
        };

        let slab = &owned[index];
        let next = slab.meta.load().next();
        slab.meta
            .store(slab::owned::Meta::new(None, size::Class::Small(class)));
        unsafe { &mut *slab.free.get() }.fill(class.count());

        let version = shared[index].meta.load().version();
        shared[index].meta.store(slab::shared::Meta::new(
            version.next(),
            size::Class::Small(class),
        ));

        self.r#sized[class].set(Some(index));
        self.r#unsized.set(next);

        true
    }

    #[cold]
    pub(crate) fn sized_to_unsized(
        &mut self,
        slabs: &slab::Slice<slab::Owned>,
        class: size::Small,
        index: slab::Index,
    ) {
        let next = slabs[index].meta.load().next();

        let mut walk = self.r#sized[class].peek().unwrap();

        if walk == index {
            self.r#sized[class].set(next);
        } else {
            let prev = loop {
                match slabs[walk].meta.load().next() {
                    None => panic!("Removing non-existent slab"),
                    Some(next) if next == index => break walk,
                    Some(next) => walk = next,
                }
            };

            slabs[prev]
                .meta
                .store(slab::owned::Meta::new(next, size::Class::Small(class)));
        };

        self.r#unsized.push(slabs, index, None);
    }
}
