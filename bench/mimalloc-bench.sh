#!/usr/bin/env bash

# https://stackoverflow.com/questions/59895/how-do-i-get-the-directory-where-a-bash-script-is-located-from-within-the-script
readonly root=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

readonly mimalloc_bench="$root/../extern/mimalloc-bench"

cd $mimalloc_bench/out/bench

../../bench.sh --external=cxlalloc.txt mi2 je cxl-shm r allt

cp benchres.csv $root/
