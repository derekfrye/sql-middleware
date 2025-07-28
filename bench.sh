#!/bin/bash

# Wrapper script for running benchmarks with different row counts
# Usage: ./bench.sh [number_of_rows]
# Example: ./bench.sh 10000

echo "=== SQL Middleware Database Benchmarks ==="
echo "Available benchmarks: SQLite, PostgreSQL"
echo "Note: LibSQL benchmarks are disabled due to threading issues with Criterion"
echo

if [ $# -eq 0 ]; then
    echo "Running benchmarks with default row count (50,000)"
    cargo bench
elif [ $# -eq 1 ]; then
    if [[ $1 =~ ^[0-9]+$ ]]; then
        echo "Running benchmarks with $1 rows"
        BENCH_ROWS=$1 cargo bench
    else
        echo "Error: Please provide a valid number"
        echo "Usage: $0 [number_of_rows]"
        echo "Example: $0 10000"
        exit 1
    fi
else
    echo "Usage: $0 [number_of_rows]"
    echo "Example: $0 10000"
    echo
    echo "Available databases:"
    echo "  - SQLite: Fast file-based database"
    echo "  - PostgreSQL: Full-featured embedded database (slower due to setup/teardown)"
    echo "  - LibSQL: Currently disabled due to threading conflicts in benchmark environment"
    exit 1
fi