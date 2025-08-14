#!/usr/bin/env bash

readonly ROOT=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

set -o errexit
set -o nounset
set -o pipefail

cargo build \
    --release \
    --package cxlalloc-bench \
    --no-default-features \
    --features allocator-mimalloc \
    --features allocator-cxlalloc \
    --features recover-shm

sudo "$ROOT/../target/release/cxlalloc-bench" "$ROOT/../cxlalloc-bench/workloads/ablation-hwcc.toml"

cargo build \
    --release \
    --package cxlalloc-bench \
    --no-default-features \
    --features allocator-mimalloc \
    --features allocator-cxlalloc \
    --features recover-shm \
    --features cxl-mcas

sudo "$ROOT/../target/release/cxlalloc-bench" "$ROOT/../cxlalloc-bench/workloads/ablation-mcas.toml"
