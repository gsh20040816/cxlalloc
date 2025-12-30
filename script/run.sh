#!/usr/bin/env bash

set -o errexit
set -o nounset
set -o pipefail

readonly ROOT=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$ROOT/.."

rm /dev/shm/* || true

cargo build --release --package cxlalloc-bench --quiet --frozen

cargo run --release --package cxlalloc-bench --quiet --frozen -- "$1"
