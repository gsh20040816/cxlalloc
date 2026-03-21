# cxlalloc

This is the primary software artifact for [Cxlalloc: Safe and Efficient Memory Allocation for a CXL Pod](https://dl.acm.org/doi/10.1145/3779212.3790149).

- Main libraries
    - `cxlalloc`: core allocation logic with no global state (i.e., no signal handler or global allocator instance)
    - `cxlalloc-static`: wrapper around `cxlalloc` with global state for C FFI (header in `cxlalloc-static/include`)
    - `cxlalloc-global`: wrapper around `cxlalloc` with global state for Rust
- Testing and benchmarking
    - `cxlalloc-bench`: integration of baseline allocators with [shm-bench](https://github.com/nwtnni/shm-bench/) benchmark harness
    - `cxlalloc-recover`: recovery time benchmarks with [memento](https://github.com/kaist-cp/memento) (Figure 7 in paper)
    - `cxlalloc-test`: basic multi-process allocation test harness
- Miscellaneous
    - `crash`: utilities for thread crash tests
    - `extern`: git submodules for external dependencies
    - `mCAS`: memory-based compare-and-swap FPGA implementation
    - `plot`: various plotting scripts, typically reading `result.ndjson` output from `cxlalloc-bench`
    - `script`: scripts for setting up [Chameleon](https://www.chameleoncloud.org/) cloud instance and running benchmarks
    - `twitter`: script to convert [memcached trace data](https://iotta.snia.org/traces/key-value/28652) to parquet
- Other dependencies (not published on crates.io)
    - [shm](https://github.com/nwtnni/shm): utilities for multi-process shm programs (which I thought I'd be writing more of in the future)
    - [shm-bench](https://github.com/nwtnni/shm-bench/): multi-process benchmark harness
    - [ribbit](github.com/nwtnni/ribbit): procdeural macro for bit-packed structs
    - [ycsb](github.com/nwtnni/ycsb): port of [YCSB workload generator](https://github.com/brianfrankcooper/YCSB/blob/8b2ecaf9c876d930096637e539c9725b5c3ba950/core/src/main/java/site/ycsb/workloads/CoreWorkload.java)

# Building

This repository uses [nix](https://nixos.org/) to set up a reproducible development environment,
but doesn't yet expose the final artifacts as a nix output.

# Known issues

- Strongly typed Rust allocation API isn't finalized (untyped C interface should be stable)
- Recoverable/detectable allocation API is in the middle of a refactor
    - PM allocators seem to provide an arbitrary number of application roots--can't we have a single user-defined type as a root?
    - Previously we tried a link/unlink API (where user provides destination to transactionally write allocated pointer), but this entangles the allocator and application pointer representations, and requires separate code paths for recoverable and non-recoverable APIs. Intending to switch to a detectable API instead (e.g., `fn detect_allocate(pointer: NonNull<T>) -> bool`), which seems easier to compose.
