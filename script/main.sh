#!/usr/bin/env bash

set -o errexit
set -o nounset
set -o pipefail

readonly ROOT=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$ROOT"

rm /dev/shm/* || true

cargo build --release --package cxlalloc-bench

cargo run \
    --release \
    --package cxlalloc-bench \
    -- \
    cxlalloc-bench/workloads/main.toml
