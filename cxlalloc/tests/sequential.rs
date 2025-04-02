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
    let raw = raw::Raw::builder().size_small(1 << 34).build("").unwrap();
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
        let pointer = NonNull::new(allocator.allocate_untyped(8)).unwrap();
        let small = pointer.cast::<u64>().as_mut();

        *small = 5;
        assert_eq!(*small, 5);
        assert!(allocator.class_untyped(pointer) >= 8);
        allocator.free_untyped(pointer);
    })
}

#[test]
fn huge() {
    with_allocator(|allocator| unsafe {
        const SIZE: usize = 1 << 30;

        let pointer = NonNull::new(allocator.allocate_untyped(SIZE)).unwrap();
        let huge = pointer.cast::<[u8; SIZE]>().as_mut();

        for i in 0..SIZE / PAGE {
            huge[i * PAGE] = i as u8;
        }

        assert!(allocator.class_untyped(pointer) >= SIZE);
        allocator.free_untyped(pointer);
    })
}

proptest! {
    #[test]
    fn single(size in 1usize..(1 << 8usize)) {
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
        Concrete<1>
    );

    #[test]
    fn concurrent(
        sequential
        1..1000
        =>
        Concrete<2>
    );
}

struct Abstract<const THREADS: usize>;

#[derive(Copy, Clone, Debug)]
enum Transition {
    Allocate {
        thread: usize,
        id: usize,
        size: usize,
    },
    Free {
        thread: usize,
        id: usize,
    },
}

impl<const THREADS: usize> ReferenceStateMachine for Abstract<THREADS> {
    type State = [HashMap<usize, usize>; THREADS];
    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        Just(std::array::from_fn(|_| HashMap::new())).boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        let state = state.clone();

        (0..THREADS)
            .prop_flat_map(move |thread| {
                let id = state[thread].len();
                let allocate = (1usize..1 << 8usize).prop_map(move |size| Transition::Allocate {
                    thread,
                    id,
                    size,
                });

                if state[thread].is_empty() {
                    return allocate.boxed();
                }

                let ids = state[thread].keys().copied().collect::<Vec<_>>();
                prop_oneof![
                    allocate,
                    proptest::sample::select(ids)
                        .prop_map(move |id| Transition::Free { thread, id }),
                ]
                .boxed()
            })
            .boxed()
    }

    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::Allocate {
                thread,
                id,
                size: _,
            } => !state[*thread].contains_key(id),
            Transition::Free { thread, id } => state[*thread].contains_key(id),
        }
    }

    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::Allocate { thread, id, size } => {
                state[*thread].insert(*id, *size);
                state
            }
            Transition::Free { thread, id } => {
                state[*thread].remove(id);
                state
            }
        }
    }
}

struct Concrete<const THREADS: usize> {
    raw: cxlalloc::Raw,
    allocations: [HashMap<usize, (NonNull<u8>, usize)>; THREADS],
}

impl<const THREADS: usize> StateMachineTest for Concrete<THREADS> {
    type SystemUnderTest = Self;
    type Reference = Abstract<THREADS>;

    fn init_test(
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        assert!(ref_state.iter().all(|state| state.is_empty()));
        Self {
            raw: raw::Raw::builder().size_small(1 << 34).build("").unwrap(),
            allocations: std::array::from_fn(|_| HashMap::new()),
        }
    }

    fn apply(
        mut state: Self::SystemUnderTest,
        _: &<Self::Reference as ReferenceStateMachine>::State,
        transition: <Self::Reference as ReferenceStateMachine>::Transition,
    ) -> Self::SystemUnderTest {
        match transition {
            Transition::Allocate { thread, id, size } => {
                let mut allocator = state
                    .raw
                    .allocator::<(), ()>(unsafe { cxlalloc::thread::Id::new(thread as u16) });

                let pointer = allocator.allocate_untyped(size);

                unsafe { libc::memset(pointer, (id ^ size) as _, size) };

                let pointer = NonNull::new(pointer).unwrap().cast::<u8>();

                assert!(state.allocations[thread]
                    .insert(id, (pointer, size))
                    .is_none());
            }
            Transition::Free { thread, id } => {
                let mut allocator = state
                    .raw
                    .allocator::<(), ()>(unsafe { cxlalloc::thread::Id::new(thread as u16) });

                let (address, size) = state.allocations[thread].remove(&id).unwrap();
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
