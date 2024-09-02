use core::marker::PhantomData;
use std::fmt::Debug;

use crate::atomic::Atomic;
use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::atomic::Version;
use crate::atomic::Versioned;
use crate::thread;

pub trait Transfer {
    type State: Packed;
    type Context: ?Sized;

    type Write: Copy + Packed + Debug;
    type Input: Packed + NonZero;

    type Read: Copy + Packed + Debug;
    type Output: Packed + NonZero;
    type Abort: Debug;

    fn try_read(
        &self,
        context: &Self::Context,
        operation: Self::Read,
        state: Self::State,
    ) -> Result<Self::Output, Self::Abort>;

    fn finish_read(
        &self,
        context: &Self::Context,
        operation: Self::Read,
        state: Self::State,
    ) -> Self::State;

    fn interpose_write(
        &self,
        context: &Self::Context,
        operation: Self::Write,
        state: Self::State,
        staged: &Self::Input,
    );

    fn finish_write(
        &self,
        context: &Self::Context,
        operation: Self::Write,
        staged: Self::Input,
    ) -> Self::State;

    fn claim(&self) -> &Claim<Self::Read, Self::Write>;
    fn state(&self) -> &State<Self::State>;
}

pub trait TransferExt: Transfer {
    fn read(
        &self,
        context: &<Self as Transfer>::Context,
        stages: &thread::Array<Stage>,
        thread_id: &mut thread::Id,
        operation: <Self as Transfer>::Read,
        version: Option<Version>,
    ) -> Result<<Self as Transfer>::Output, <Self as Transfer>::Abort> {
        if let Some(version) = version {
            return self::read(self, context, stages, thread_id, operation, version);
        }

        if let Some(current) = self.claim().0.load().transpose() {
            complete(self, context, stages, current);
        }

        let staged = stages[thread_id].load_versioned::<Option<<Self as Transfer>::Output>>();

        match staged.inner() {
            Some(output) => Ok(output),
            None => self::read(
                self,
                context,
                stages,
                thread_id,
                operation,
                staged.version(),
            ),
        }
    }

    fn write(
        &self,
        context: &<Self as Transfer>::Context,
        stages: &thread::Array<Stage>,
        thread_id: &mut thread::Id,
        operation: <Self as Transfer>::Write,
        staged: Option<Versioned<<Self as Transfer>::Input>>,
    ) {
        if let Some(staged) = staged {
            return self::write(self, context, stages, thread_id, operation, staged);
        }

        if let Some(current) = self.claim().0.load().transpose() {
            self::complete(self, context, stages, current);
        }

        let staged = stages[thread_id].load_versioned::<Option<<Self as Transfer>::Input>>();

        match staged.inner() {
            None => (),
            Some(input) => self::write(
                self,
                context,
                stages,
                thread_id,
                operation,
                Versioned::new(input, staged.version()),
            ),
        }
    }
}

impl<T: Transfer + ?Sized> TransferExt for T {}

#[repr(transparent)]
pub struct State<T>(Atomic<Versioned<T>>);

#[repr(C)]
pub struct Stage(Atomic<u64>);

#[repr(transparent)]
#[derive(Debug)]
pub struct Claim<R, W>(Atomic<Versioned<Option<ClaimInner<R, W>>>>);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ClaimInner<R, W> {
    // pub(crate) version_local: Wrapping<u16>,
    // pub(crate) thread_id: usize,
    // pub(crate) operation: Operation<R, W>,
    value: u64,
    _read: PhantomData<R>,
    _write: PhantomData<W>,
}

unsafe impl<R: Packed, W: Packed> Packed for ClaimInner<R, W> {
    const BITS: u8 = 48;

    fn pack(&self) -> u64 {
        self.value
    }

    fn unpack(value: u64) -> Self {
        Self {
            value,
            _read: PhantomData,
            _write: PhantomData,
        }
    }
}

impl<R: Packed, W: Packed> ClaimInner<R, W> {
    fn new(version_local: Version, operation: Operation<R, W>, thread_id: &mut thread::Id) -> Self {
        Self {
            value: (version_local.pack() << 32) | (operation.pack() << 16) | thread_id.pack(),
            _read: PhantomData,
            _write: PhantomData,
        }
    }

    fn version_local(&self) -> Version {
        Version::unpack(self.value >> 32)
    }

    fn operation(&self) -> Operation<R, W> {
        Operation::<R, W>::unpack(self.value >> 16)
    }

    fn thread_id(&self) -> thread::Id {
        thread::Id::unpack(self.value)
    }
}

/// # Safety
///
/// Bits 0..16 are `thread::Id: NonZero`.
unsafe impl<R, W> NonZero for ClaimInner<R, W> {}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Operation<R, W> {
    Read(R),
    Write(W),
}

unsafe impl<R: Packed, W: Packed> Packed for Operation<R, W> {
    const BITS: u8 = Self::INNER + 1;

    fn pack(&self) -> u64 {
        match self {
            Operation::Read(read) => (0 << Self::INNER) | read.pack(),
            Operation::Write(write) => (1 << Self::INNER) | write.pack(),
        }
    }

    fn unpack(value: u64) -> Self {
        match value & (1 << Self::INNER) > 0 {
            false => Operation::Read(R::unpack(value)),
            true => Operation::Write(W::unpack(value)),
        }
    }
}

impl<R: Packed, W: Packed> Operation<R, W> {
    const INNER: u8 = if R::BITS > W::BITS { R::BITS } else { W::BITS };
}

fn read<T: Transfer + ?Sized>(
    global: &T,
    context: &T::Context,
    stages: &thread::Array<Stage>,
    thread_id: &mut thread::Id,
    operation: T::Read,
    version: Version,
) -> Result<T::Output, T::Abort> {
    let claim = ClaimInner::new(version, Operation::Read(operation), thread_id);

    let output = loop {
        let state = global.state().0.load();
        let output = global
            .try_read(context, operation, state.inner())
            .map_err(|abort| {
                log::debug!("[{}]: Aborted {:?}", std::any::type_name::<T>(), claim,);
                abort
            })?;

        if apply(
            global,
            context,
            stages,
            Versioned::new(claim, state.version()),
        ) {
            break output;
        }
    };

    Ok(output)
}

fn write<T: Transfer + ?Sized>(
    global: &T,
    context: &T::Context,
    stages: &thread::Array<Stage>,
    thread_id: &mut thread::Id,
    operation: T::Write,
    staged: Versioned<T::Input>,
) {
    let input = staged.inner();
    let claim = ClaimInner::new(staged.version(), Operation::Write(operation), thread_id);

    loop {
        let state = global.state().0.load();
        global.interpose_write(context, operation, state.inner(), &input);

        if apply(
            global,
            context,
            stages,
            Versioned::new(claim, state.version()),
        ) {
            break;
        }
    }
}

fn apply<T: Transfer + ?Sized>(
    global: &T,
    context: &T::Context,
    stages: &thread::Array<Stage>,
    claim: Versioned<ClaimInner<T::Read, T::Write>>,
) -> bool {
    let previous = global
        .claim()
        .0
        .compare_exchange(claim.map(|_| None), claim.map(Option::Some));

    let current = match &previous {
        Ok(_) => {
            log::debug!("[{}]: Installed {:?}", std::any::type_name::<T>(), claim,);
            claim
        }
        Err(current) => match current.inner() {
            None => {
                // An operation interleaved between creating this claim and CASing it
                log::debug!("[{}]: Restarting {:?}", std::any::type_name::<T>(), claim);
                return false;
            }
            Some(claim) => {
                log::debug!("[{}]: Helping {:?}", std::any::type_name::<T>(), claim);
                Versioned::new(claim, current.version())
            }
        },
    };

    complete(global, context, stages, current);

    // Successful if and only if claim CAS succeeds
    previous.is_ok()
}

fn complete<T: Transfer + ?Sized>(
    global: &T,
    context: &T::Context,
    stages: &thread::Array<Stage>,
    current: Versioned<ClaimInner<T::Read, T::Write>>,
) {
    let version_global = current.version();
    let claim = current.inner();
    let version_local = claim.version_local();
    let thread_id = claim.thread_id();
    let operation = claim.operation();

    'early: {
        match operation {
            Operation::Write(operation) => {
                let staged = stages[&thread_id].load_versioned::<Option<T::Input>>();

                // Staging area has already been cleared
                if staged.version() != version_local {
                    break 'early;
                }

                let old = global.state().0.load();
                if old.version() == version_global {
                    let input = staged.inner().unwrap();

                    let new = global.finish_write(context, operation, input);
                    let new = Versioned::new(new, old.next_version());

                    let _ = global.state().0.compare_exchange(old, new);
                }

                let _ = stages[&thread_id].compare_exchange(staged, Option::<T::Input>::None);
            }
            Operation::Read(operation) => {
                let old = global.state().0.load();

                // Global state has already been updated
                if old.version() != version_global {
                    break 'early;
                }

                let output = global.try_read(context, operation, old.inner()).unwrap();
                let _ = stages[&thread_id].compare_exchange(
                    Versioned::<Option<T::Output>>::new(None, version_local),
                    Some(output),
                );

                let new = global.finish_read(context, operation, old.inner());
                let new = Versioned::new(new, old.next_version());

                let _ = global.state().0.compare_exchange(old, new);
            }
        }
    }

    let _ = global.claim().0.compare_exchange(
        current.map(Option::Some),
        Versioned::new(None, current.next_version()),
    );
}

impl Stage {
    pub fn load<T: Packed>(&self) -> T {
        self.load_versioned().inner()
    }

    fn load_versioned<T: Packed>(&self) -> Versioned<T> {
        Versioned::<T>::unpack(self.0.load())
    }

    /// Unconditionally store `value` in staging area, returning the new version.
    pub fn store_versioned<T: Packed>(&self, value: T) -> Versioned<T> {
        let version = Versioned::<T>::unpack(self.0.load()).next_version();
        // FIXME: hack to avoid requiring Copy or Clone
        let saved = T::unpack(value.pack());
        self.0.store(Versioned::new(value, version).pack());
        Versioned::new(saved, version)
    }

    fn compare_exchange<T: Packed>(&self, old: Versioned<T>, new: T) -> Result<(), ()> {
        let new = Versioned::new(new, old.next_version()).pack();
        let old = old.pack();
        self.0
            .compare_exchange(old, new)
            .map(|_| ())
            .map_err(|_| ())
    }
}
