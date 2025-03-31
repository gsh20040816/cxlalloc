#!/usr/bin/env bash

readonly ROOT=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$ROOT"

rm /dev/shm/{barrier,tt*,xm*,ycsb*,mc*,ms*,acked,index,ebr}

cargo build --release --package cxlalloc-bench

cargo run \
    --release \
    --package cxlalloc-bench \
    -- \
    --output "ycsb-load.ndjson" \
    ycsb \
    load

cargo run \
    --release \
    --package cxlalloc-bench \
    -- \
    --output "ycsb-d.ndjson" \
    ycsb \
    d

cargo run \
    --release \
    --package cxlalloc-bench \
    -- \
    --output "thread-test.ndjson" \
    thread-test

cargo run \
    --release \
    --package cxlalloc-bench \
    -- \
    --output "xmalloc.ndjson" \
    xmalloc

cargo run \
    --release \
    --package cxlalloc-bench \
    -- \
    --output "memcached.ndjson" \
    memcached

cargo run \
    --release \
    --package cxlalloc-bench \
    -- \
    --output "memcached.ndjson" \
    memcached \
    --operation-count 1000000 \
    --trace "twitter/cluster37.000.parquet"

