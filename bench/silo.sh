#!/usr/bin/env bash

# https://stackoverflow.com/questions/59895/how-do-i-get-the-directory-where-a-bash-script-is-located-from-within-the-script
readonly root=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

readonly prefix="$root/../extern/silo/out-perf.masstree."
readonly time=30

for allocator in cxlalloc cxl-shm mimalloc ralloc; do
    for scale_factor in 1 10 20 30 40; do
        $prefix$allocator/benchmarks/dbtest \
            --verbose \
            --bench tpcc \
            --pin-cpus \
            --num-threads 40 \
            --scale-factor $scale_factor \
            --runtime $time \
            2>&1 | tee "tpcc-$allocator-t40-sf$scale_factor.log"
    done

    for threads in 1 10 20 30 40; do
        $prefix$allocator/benchmarks/dbtest \
            --verbose \
            --bench tpcc \
            --pin-cpus \
            --num-threads $threads \
            --scale-factor 40 \
            --runtime $time \
            2>&1 | tee "tpcc-$allocator-t$threads-sf40.log"
    done

    $prefix$allocator/benchmarks/dbtest \
        --verbose \
        --bench ycsb \
        --pin-cpus \
        --num-threads 40 \
        --scale-factor 40 \
        --runtime $time \
        --bench-opts \
        --workload-mix=50,50,0,0 \
        2>&1 | tee "ycsb-a-$allocator-t40-sf$scale_factor.log"

    $prefix$allocator/benchmarks/dbtest \
        --verbose \
        --bench ycsb \
        --pin-cpus \
        --num-threads 40 \
        --scale-factor 40 \
        --runtime $time \
        --bench-opts \
        --workload-mix=95,5,0,0 \
        2>&1 | tee "ycsb-b-$allocator-t40-sf$scale_factor.log"

done
