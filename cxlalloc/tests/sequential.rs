use core::ptr::NonNull;
use std::collections::HashMap;

use proptest::prelude::*;

use cxlalloc::raw;
use cxlalloc::Allocator;
use proptest_state_machine::prop_state_machine;
use proptest_state_machine::ReferenceStateMachine;
use proptest_state_machine::StateMachineTest;

const PAGE: usize = 4096;

fn with_allocator<F: FnOnce(&mut Allocator)>(apply: F) {
    let _ = env_logger::try_init();
    let raw = raw::Builder::default().build("").unwrap();
    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let mut allocator = raw.allocator(id);
    apply(&mut allocator)
}

#[test]
fn create() {
    with_allocator(|_| ())
}

#[test]
fn small() {
    with_allocator(|allocator| unsafe {
        let small = allocator
            .allocate_untyped(8)
            .cast::<u64>()
            .as_mut()
            .unwrap();

        *small = 5;
        assert_eq!(*small, 5);

        allocator.free_untyped(NonNull::from(small).cast());
    })
}

#[test]
fn huge() {
    with_allocator(|allocator| unsafe {
        const SIZE: usize = 1 << 30;

        let huge = allocator
            .allocate_untyped(SIZE)
            .cast::<[u8; SIZE]>()
            .as_mut()
            .unwrap();

        for i in 0..SIZE / PAGE {
            huge[i * PAGE] = i as u8;
        }

        allocator.free_untyped(NonNull::from(huge).cast());
    })
}

proptest! {
    #[test]
    fn single(size in 1usize..(1 << 20usize)) {
        with_allocator(|allocator| unsafe {
            let allocation = allocator.allocate_untyped(size);
            allocator.free_untyped(NonNull::new(allocation).unwrap());
        })
    }
}

prop_state_machine! {
    #[test]
    fn sequential(
        sequential
        1..1000
        =>
        Concrete
    );
}

struct Abstract;

#[derive(Copy, Clone, Debug)]
enum Transition {
    Allocate { id: usize, size: usize },
    Free { id: usize },
}

impl ReferenceStateMachine for Abstract {
    type State = HashMap<usize, usize>;
    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        Just(HashMap::new()).boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        let id = state.len();
        let allocate =
            (1usize..1 << 10usize).prop_map(move |size| Transition::Allocate { id, size });

        if state.is_empty() {
            return allocate.boxed();
        }

        let ids = state.keys().copied().collect::<Vec<_>>();
        prop_oneof![
            allocate,
            proptest::sample::select(ids).prop_map(|id| Transition::Free { id }),
        ]
        .boxed()
    }

    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::Allocate { id, size: _ } => !state.contains_key(id),
            Transition::Free { id } => state.contains_key(id),
        }
    }

    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::Allocate { id, size } => {
                state.insert(*id, *size);
                state
            }
            Transition::Free { id } => {
                state.remove(id);
                state
            }
        }
    }
}

struct Concrete {
    raw: cxlalloc::Raw,
    allocations: HashMap<usize, (NonNull<u8>, usize)>,
}

impl StateMachineTest for Concrete {
    type SystemUnderTest = Self;
    type Reference = Abstract;

    fn init_test(
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        assert!(ref_state.is_empty());
        Self {
            raw: raw::Builder::default().build("").unwrap(),
            allocations: HashMap::new(),
        }
    }

    fn apply(
        mut state: Self::SystemUnderTest,
        _: &<Self::Reference as ReferenceStateMachine>::State,
        transition: <Self::Reference as ReferenceStateMachine>::Transition,
    ) -> Self::SystemUnderTest {
        let mut allocator = state
            .raw
            .allocator::<(), ()>(unsafe { cxlalloc::thread::Id::new(0) });

        match transition {
            Transition::Allocate { id, size } => {
                let pointer = allocator.allocate_untyped(size);

                unsafe { libc::memset(pointer, (id ^ size) as _, size) };

                let pointer = NonNull::new(pointer).unwrap().cast::<u8>();
                assert!(state.allocations.insert(id, (pointer, size)).is_none());
            }
            Transition::Free { id } => {
                let (address, size) = state.allocations.remove(&id).unwrap();
                assert!(allocator.class_untyped(address.cast()) >= size);

                for i in 0..size {
                    assert_eq!(unsafe { *address.byte_add(i).as_ref() }, (id ^ size) as u8);
                }

                unsafe {
                    allocator.free_untyped(address.cast());
                }
            }
        }

        state
    }
}
