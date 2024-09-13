use core::alloc::Layout;
use core::convert::Infallible;
use core::ops::Index;
use core::ops::Range;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::atomic::Version;
use crate::atomic::Versioned;
use crate::raw;
use crate::root;
use crate::slab;
use crate::thread;
use crate::transfer;
use crate::transfer::TransferExt;
use crate::Transfer;
use crate::SIZE_PAGE;

pub(crate) struct Shared<'raw> {
    capacity: u32,
    meta: &'raw Meta<'raw>,
    pub(crate) slabs: slab::Slice<'raw, slab::Shared>,
}

impl<'raw> Shared<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::new::<Meta>()
            .extend(slab::Slice::<slab::Shared>::layout(slab_count).unwrap())
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

    pub(crate) fn allocate(
        &self,
        id: thread::Id,
        count: u16,
        version: Option<Version>,
    ) -> Result<Range<slab::Index>, Epoch> {
        self.meta
            .read(
                &self.capacity,
                &self.meta.stages,
                id,
                Read::Allocate(count),
                version,
            )
            .map(|extent| extent.length())
            .map(|length| {
                let end = slab::Index::from_length(length);
                let start = slab::Index::from_length(length - u32::from(count));
                start..end
            })
    }

    pub(crate) fn push(
        &self,
        id: thread::Id,
        slabs: &slab::Slice<slab::Owned>,
        count: u16,
        staged: Option<Versioned<slab::Index>>,
    ) {
        self.meta
            .free
            .write(slabs, &self.meta.stages, id, slab::Push::new(count), staged);
    }

    pub(crate) fn pop(
        &self,
        id: thread::Id,
        slabs: &slab::Slice<slab::Owned>,
        version: Option<Version>,
    ) -> Result<slab::Index, slab::Empty> {
        if self.meta.free.is_empty() {
            return Err(slab::Empty);
        }

        self.meta
            .free
            .read(slabs, &self.meta.stages, id, slab::Pop, version)
    }
}

impl Index<root::Index> for Shared<'_> {
    type Output = Option<slab::Offset>;
    fn index(&self, index: root::Index) -> &Self::Output {
        &self.meta.roots[index]
    }
}

impl Index<thread::Id> for Shared<'_> {
    type Output = transfer::Stage;
    fn index(&self, id: thread::Id) -> &Self::Output {
        &self.meta.stages[id]
    }
}

#[repr(C)]
pub(crate) struct Meta<'raw> {
    roots: root::Array,
    free: slab::GlobalStack<'raw>,
    claim: transfer::Claim<Read, Infallible>,
    extent: transfer::State<Extent>,
    stages: thread::Array<transfer::Stage>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Extent(u64);

impl Extent {
    fn new(epoch: Epoch, length: u32) -> Self {
        Self(((epoch.0 as u64) << 32) | length as u64)
    }

    pub(crate) fn epoch(&self) -> Epoch {
        Epoch((self.0 >> 32) as u8)
    }

    pub(crate) fn length(&self) -> u32 {
        self.0 as u32
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
    fn capacity(&self, initial: u32) -> u32 {
        2u32.pow(self.0 as u32) * initial
    }
}

impl<'raw> Transfer for Meta<'raw> {
    type State = Extent;
    type Context = u32;

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
            Read::Allocate(count) if length + count as u32 <= epoch.capacity(*initial) => {
                Ok(Extent::new(epoch, length + count as u32))
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
            Read::Allocate(count) => Extent::new(epoch, length + count as u32),
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
        match self {
            #[allow(clippy::identity_op)]
            Self::Allocate(count) => (0 << 14) | *count as u64,
            Self::Extend(_) => todo!(),
        }
    }

    fn unpack(value: u64) -> Self {
        match value & (1 << 14) > 0 {
            false => Read::Allocate((value & ((1 << 14) - 1)) as u16),
            true => todo!(),
        }
    }
}
