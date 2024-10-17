#!/usr/bin/env bash

# https://stackoverflow.com/questions/59895/how-do-i-get-the-directory-where-a-bash-script-is-located-from-within-the-script
readonly root=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

readonly mimalloc_bench="$root/../extern/mimalloc-bench"

cd $mimalloc_bench/out/bench

CXL_NUMA_NODE=1 ../../bench.sh --external=cxlalloc.txt mi2 je cxl-shm r -n=10 allt
# Some problems with $PATH for these benchmarks:
# CXL_NUMA_NODE=1 ../../bench.sh --external=cxlalloc.txt mi2 je cxl-shm r -n=10 gs lua z3 rbstress

cp benchres.csv $root
