#!/usr/bin/env bash

# https://stackoverflow.com/questions/59895/how-do-i-get-the-directory-where-a-bash-script-is-located-from-within-the-script
ROOT=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

readonly prefix="$ROOT/../extern/silo/out-perf.masstree."
readonly time=30

for allocator in cxl-shm cxlalloc mimalloc ralloc; do
    for scale_factor in 1 10 20 30 40; do
        CXL_NUMA_NODE=2 $prefix$allocator/benchmarks/dbtest \
            --verbose \
            --bench tpcc \
            --num-threads 40 \
            --scale-factor $scale_factor \
            --runtime $time \
            2>&1 | tee "$allocator-t40-sf$scale_factor.log"
    done

    for threads in 1 10 20 30 40; do
        CXL_NUMA_NODE=2 $prefix$allocator/benchmarks/dbtest \
            --verbose \
            --bench tpcc \
            --num-threads $threads \
            --scale-factor 40 \
            --runtime $time \
            2>&1 | tee "$allocator-t$threads-sf40.log"
    done
done
