use core::alloc::Layout;
use core::convert::Infallible;
use core::ops::Index;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::raw;
use crate::region;
use crate::root;
use crate::slab;
use crate::thread;
use crate::transfer;
use crate::Transfer;
use crate::SIZE_PAGE;

pub(crate) struct Shared<'raw> {
    capacity: usize,
    meta: &'raw Meta,
    slabs: slab::Slice<'raw, slab::Shared>,
}

impl<'raw> Shared<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::new::<Meta>()
            .extend(Layout::array::<slab::Shared>(slab_count).unwrap())
            .unwrap()
            .0
            .align_to(SIZE_PAGE)
            .unwrap()
            .pad_to_align()
    }

    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner) -> Self {
        // FIXME: deduplicate with `layout`
        let offset = Layout::new::<Meta>()
            .extend(Layout::array::<slab::Shared>(1).unwrap())
            .unwrap()
            .1;

        Self {
            capacity: raw.capacity,
            meta: raw.shared.base().cast::<Meta>().as_ref(),
            slabs: slab::Slice::from_raw(&raw.shared, offset),
        }
    }
}

impl Index<root::Index> for Shared<'_> {
    type Output = Option<region::data::Offset>;
    fn index(&self, index: root::Index) -> &Self::Output {
        &self.meta.roots[index]
    }
}

#[repr(C)]
pub(crate) struct Meta {
    roots: root::Array,
    free: slab::GlobalStack,
    claim: transfer::Claim<Read, Infallible>,
    extent: transfer::State<Extent>,
    stages: thread::Array<transfer::Stage>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Extent(u64);

impl Extent {
    fn new(epoch: Epoch, length: usize) -> Self {
        todo!()
    }

    pub(crate) fn epoch(&self) -> Epoch {
        Epoch(self.0 as u8)
    }

    pub(crate) fn length(&self) -> usize {
        todo!()
    }
}

unsafe impl Packed for Extent {
    const BITS: u8 = 48;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
}

unsafe impl NonZero for Extent {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Epoch(u8);

impl Epoch {
    fn capacity(&self, initial: usize) -> usize {
        2usize.pow(self.0 as u32) * initial
    }
}

impl Transfer for Meta {
    type State = Extent;
    type Context = usize;

    type Write = Infallible;
    type Input = Infallible;

    type Read = Read;
    type Output = Extent;

    type Abort = Epoch;

    fn try_read(
        &self,
        initial: &Self::Context,
        operation: Self::Read,
        extent: Self::State,
    ) -> Result<Self::Output, Self::Abort> {
        let epoch = extent.epoch();
        let length = extent.length();
        match operation {
            Read::Allocate(count) if length + count as usize <= epoch.capacity(*initial) => {
                Ok(Extent::new(epoch, length + count as usize))
            }
            Read::Allocate(_) => Err(epoch),
            Read::Extend(_) => todo!(),
        }
    }

    fn finish_read(
        &self,
        _: &Self::Context,
        operation: Self::Read,
        extent: Self::State,
    ) -> Self::State {
        let epoch = extent.epoch();
        let length = extent.length();
        match operation {
            Read::Allocate(count) => Extent::new(epoch, length + count as usize),
            Read::Extend(_) => todo!(),
        }
    }

    fn interpose_write(&self, _: &Self::Context, _: Self::Write, _: Self::State, _: &Self::Input) {
        unreachable!()
    }

    fn finish_write(&self, _: &Self::Context, _: Self::Write, _: Self::Input) -> Self::State {
        todo!()
    }

    fn claim(&self) -> &transfer::Claim<Self::Read, Self::Write> {
        &self.claim
    }

    fn state(&self) -> &transfer::State<Self::State> {
        &self.extent
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum Read {
    Allocate(u16),
    Extend(Epoch),
}

unsafe impl Packed for Read {
    const BITS: u8 = 15;

    fn pack(&self) -> u64 {
        todo!()
    }

    fn unpack(value: u64) -> Self {
        todo!()
    }
}
