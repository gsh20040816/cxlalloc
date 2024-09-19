use core::convert::Infallible;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::num::Wrapping;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

pub unsafe trait Packed {
    const BITS: u8;
    const MASK: u64 = (1 << Self::BITS as u64) - 1;
    const ASSERT: () = assert!(Self::BITS > 0 && Self::BITS <= 64);
    fn pack(&self) -> u64;
    fn unpack(value: u64) -> Self;
}

pub unsafe trait NonZero {}

unsafe impl<T: Packed + NonZero> Packed for Option<T> {
    const BITS: u8 = T::BITS;
    fn pack(&self) -> u64 {
        match self {
            Some(inner) => inner.pack(),
            None => 0,
        }
    }

    fn unpack(value: u64) -> Self {
        match value {
            0 => None,
            _ => Some(T::unpack(value)),
        }
    }
}

unsafe impl Packed for u64 {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        *self
    }

    fn unpack(value: u64) -> Self {
        value
    }
}

unsafe impl Packed for u32 {
    const BITS: u8 = 32;

    fn pack(&self) -> u64 {
        *self as u64
    }

    fn unpack(value: u64) -> Self {
        value as u32
    }
}

unsafe impl Packed for Infallible {
    const BITS: u8 = 0;
    fn pack(&self) -> u64 {
        unreachable!()
    }
    fn unpack(_: u64) -> Self {
        unreachable!()
    }
}

unsafe impl NonZero for Infallible {}

#[repr(C)]
pub struct Atomic<T> {
    value: AtomicU64,
    _type: PhantomData<T>,
}

impl<T: Packed> Atomic<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: AtomicU64::new(value.pack()),
            _type: PhantomData,
        }
    }

    pub fn load(&self) -> T {
        T::unpack(self.value.load(Ordering::Acquire))
    }

    pub fn store(&self, value: T) {
        self.value.store(value.pack(), Ordering::Release)
    }

    pub fn compare_exchange(&self, old: T, new: T) -> Result<T, T> {
        self.value
            .compare_exchange(old.pack(), new.pack(), Ordering::AcqRel, Ordering::Acquire)
            .map(T::unpack)
            .map_err(T::unpack)
    }

    pub fn fetch_xor(&self, value: u64) -> u64 {
        self.value.fetch_xor(value, Ordering::AcqRel)
    }
}

impl<T: Packed + Default> Default for Atomic<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Versioned<T> {
    value: u64,
    _type: PhantomData<T>,
}

unsafe impl<T: Packed> Packed for Versioned<T> {
    const BITS: u8 = <T as Packed>::BITS + 16;

    fn pack(&self) -> u64 {
        self.value
    }

    fn unpack(value: u64) -> Self {
        Self {
            value,
            _type: PhantomData,
        }
    }
}

impl<T: Packed + Debug> Debug for Versioned<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Versioned")
            .field("version", &self.version())
            .field("value", &self.inner())
            .finish()
    }
}

/// # Safety
///
/// Bits 0..16 are occupied by `T`.
unsafe impl<T: NonZero> NonZero for Versioned<T> {}

impl<T: Packed> Versioned<T> {
    pub fn new(value: T, version: Version) -> Self {
        Self {
            value: (version.pack() << <T as Packed>::BITS) | value.pack(),
            _type: PhantomData,
        }
    }

    pub fn version(&self) -> Version {
        Version::unpack(self.value >> <T as Packed>::BITS)
    }

    pub fn next_version(&self) -> Version {
        self.version().next()
    }

    pub fn inner(&self) -> T {
        T::unpack(self.value & <T as Packed>::MASK)
    }

    pub fn map<F: FnOnce(T) -> U, U: Packed>(&self, apply: F) -> Versioned<U> {
        Versioned::new(apply(self.inner()), self.version())
    }
}

impl<T: Packed + NonZero> Versioned<Option<T>> {
    pub fn transpose(&self) -> Option<Versioned<T>> {
        self.inner()
            .map(|inner| Versioned::new(inner, self.version()))
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version(Wrapping<u16>);

impl Version {
    pub fn new(version: u16) -> Self {
        Self(Wrapping(version))
    }

    pub fn next(&self) -> Self {
        Self(self.0 + Wrapping(1))
    }
}

unsafe impl Packed for Version {
    const BITS: u8 = 16;

    fn pack(&self) -> u64 {
        self.0 .0 as u64
    }

    fn unpack(value: u64) -> Self {
        Self(Wrapping(value as u16))
    }
}
