#!/usr/bin/env bash
# WX-1 WSL2 Environment Configuration
# Sources Conda Tempest, sets ext4 target and scratch directories, sets LD_LIBRARY_PATH for PyO3

export CONDA_PREFIX="/home/j_ord/miniconda3/envs/Tempest"
export PATH="$HOME/.cargo/bin:$CONDA_PREFIX/bin:$PATH"
export LD_LIBRARY_PATH="$CONDA_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# Test and scratch locations: dataset/benchmark files MUST stay on D: (/mnt/d) so WSL (C: drive) never fills up
export WX1_TESTS_DIR="/home/j_ord/WX-1-TESTS"
export WX1_SCRATCH="/mnt/d/Weer/Uitvoer/data/wx1_bench_scratch"
export CARGO_TARGET_DIR="/home/j_ord/WX-1-TESTS/target"

mkdir -p "$WX1_TESTS_DIR" "$WX1_SCRATCH" "$CARGO_TARGET_DIR" "/home/j_ord/WX-1-TESTS/reports/logs"
