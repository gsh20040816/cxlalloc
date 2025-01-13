use core::sync::atomic;

use crate::size;
use crate::view;
use crate::view::data;

impl view::Huge<'_> {
    pub(crate) fn free(&self, offset_allocation: data::Offset<size::Huge>) {
        let slot = offset_allocation.into_index();
        let owner = self[slot].load().unwrap();
        let mut walk = &self[owner];

        while walk.offset != offset_allocation {
            walk = walk.next.as_ref().unwrap();
        }

        walk.free.store(true, atomic::Ordering::Relaxed);
    }
}
