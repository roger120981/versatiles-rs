#!/usr/bin/env bash

PROJECT_DIR="$(dirname "$0")/.."
source "$PROJECT_DIR/scripts/env-gdal.sh"
cargo llvm-cov test --bins --all-features --tests --lcov --output-path "$PROJECT_DIR/lcov.info" $1
cargo llvm-cov report
