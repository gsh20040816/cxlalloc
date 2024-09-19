use core::alloc::Layout;
use core::convert::Infallible;
use core::ops::Index;
use core::ops::Range;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::atomic::Version;
use crate::atomic::Versioned;
use crate::extend::Epoch;
use crate::raw;
use crate::root;
use crate::slab;
use crate::thread;
use crate::transfer;
use crate::transfer::TransferExt;
use crate::Barrier;
use crate::Transfer;
use crate::SIZE_PAGE;

pub(crate) struct Shared<'raw> {
    capacity: u32,
    process_count: usize,
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
            process_count: raw.process_count,
            meta: raw.shared.base().cast::<Meta>().as_ref(),
            slabs: slab::Slice::from_raw(&raw.shared, offset),
        }
    }

    pub(crate) fn barrier(&self) -> &Barrier {
        &self.meta.barrier
    }

    pub(crate) fn request(&self) -> Option<Request> {
        self.meta.map.claim.read()
    }

    pub(crate) fn extend(
        &self,
        id: thread::Id,
        epoch: Epoch,
        version: Option<Version>,
    ) -> Result<(), Epoch> {
        self.meta
            .map
            .read(
                &self.process_count,
                &self.meta.stages,
                id,
                Request::Extend(epoch),
                version,
            )
            .map(drop)
    }

    pub(crate) fn allocate(
        &self,
        id: thread::Id,
        count: u16,
        version: Option<Version>,
    ) -> Result<Range<slab::Index>, Epoch> {
        self.meta
            .bump
            .read(
                &(self.capacity, self.meta.map.epoch.load()),
                &self.meta.stages,
                id,
                Allocate(count),
                version,
            )
            .map(|length| {
                let end = slab::Index::from_length(length);
                let start = slab::Index::from_length(Length(length.0 - u32::from(count)));
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
    stages: thread::Array<transfer::Stage>,

    bump: Bump,
    map: Map,

    barrier: Barrier,
}

struct Bump {
    claim: transfer::Claim<Allocate, Infallible>,
    length: transfer::State<Length>,
}

impl Transfer for Bump {
    type State = Length;
    type Context = (u32, Epoch);

    type Write = Infallible;
    type Input = Infallible;

    type Read = Allocate;
    type Output = Length;

    type Abort = Epoch;

    fn try_read(
        &self,
        (initial, epoch): &Self::Context,
        Allocate(count): Self::Read,
        Length(length): Self::State,
    ) -> Result<Self::Output, Self::Abort> {
        if length + count as u32 <= epoch.total(*initial) {
            Ok(Length(length + count as u32))
        } else {
            Err(*epoch)
        }
    }

    fn finish_read(
        &self,
        _: &Self::Context,
        Allocate(count): Self::Read,
        length: Versioned<Self::State>,
    ) -> Self::State {
        Length(length.inner().0 + count as u32)
    }

    fn interpose_write(&self, _: &Self::Context, _: Self::Write, _: Self::State, _: &Self::Input) {
        unreachable!()
    }

    fn finish_write(&self, _: &Self::Context, _: Self::Write, _: Self::Input) -> Self::State {
        unreachable!()
    }

    fn claim(&self) -> &transfer::Claim<Self::Read, Self::Write> {
        &self.claim
    }

    fn state(&self) -> &transfer::State<Self::State> {
        &self.length
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Length(u32);

impl From<Length> for u32 {
    fn from(Length(length): Length) -> Self {
        length
    }
}

unsafe impl Packed for Length {
    const BITS: u8 = 32;

    fn pack(&self) -> u64 {
        self.0 as u64
    }

    fn unpack(value: u64) -> Self {
        Self(value as u32)
    }
}

// Note: initially is zero, but we only
// serialize and deserialize in Transfer::Output,
// in the staging area, and there it is always nonzero.
unsafe impl NonZero for Length {}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Allocate(u16);

unsafe impl Packed for Allocate {
    const BITS: u8 = 15;

    fn pack(&self) -> u64 {
        self.0 as u64
    }

    fn unpack(value: u64) -> Self {
        Self((value & Self::MASK) as u16)
    }
}

struct Map {
    claim: transfer::Claim<Request, Infallible>,
    epoch: transfer::State<Epoch>,
    barrier: Barrier,
}

impl Transfer for Map {
    type State = Epoch;
    type Context = usize;

    type Write = Infallible;
    type Input = Infallible;

    type Read = Request;
    type Output = slab::Index;

    type Abort = Epoch;

    fn try_read(
        &self,
        _: &Self::Context,
        request: Self::Read,
        state: Self::State,
    ) -> Result<Self::Output, Self::Abort> {
        match request {
            Request::Map(_) => todo!(),
            // Caller should not use this index--cannot return `None` because
            // we need `Output: NonZero` to support `Option<Output>`.
            Request::Extend(epoch) if epoch == state => Ok(slab::Index::dangling()),
            Request::Extend(_) => Err(state),
        }
    }

    fn finish_read(
        &self,
        process_count: &Self::Context,
        request: Self::Read,
        state: Versioned<Self::State>,
    ) -> Self::State {
        self.barrier.request(*process_count, state.version());

        match request {
            Request::Map(_) => state.inner(),
            Request::Extend(_) => state.inner().next(),
        }
    }

    fn interpose_write(&self, _: &Self::Context, _: Self::Write, _: Self::State, _: &Self::Input) {
        unreachable!()
    }

    fn finish_write(&self, _: &Self::Context, _: Self::Write, _: Self::Input) -> Self::State {
        unreachable!()
    }

    fn claim(&self) -> &transfer::Claim<Self::Read, Self::Write> {
        &self.claim
    }

    fn state(&self) -> &transfer::State<Self::State> {
        &self.epoch
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum Request {
    Map(u16),
    Extend(Epoch),
}

unsafe impl Packed for Request {
    const BITS: u8 = 15;

    fn pack(&self) -> u64 {
        match self {
            Request::Map(count) => *count as u64,
            Request::Extend(epoch) => epoch.pack() | (1 << Self::BITS),
        }
    }

    fn unpack(value: u64) -> Self {
        match value & (1 << Self::BITS) > 0 {
            false => Self::Map(value as u16),
            true => Self::Extend(Epoch::unpack(value)),
        }
    }
}
