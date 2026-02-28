#!/bin/bash
# Benchmark all slow PDF datasets
# Usage: ./scripts/bench_slow_datasets.sh [binary_path] [timeout_seconds]
# Default binary: ./target/release/examples/extract_text_simple
# Default timeout: 30 seconds per PDF

set -euo pipefail

BINARY="${1:-./target/release/examples/extract_text_simple}"
TIMEOUT="${2:-30}"
TEST_DIR="${3:-$HOME/projects/pdf_oxide_tests}"

DATASETS=("pdfs_slow/slow_pdfs" "pdfs_slow2" "pdfs_slow3" "pdfs_slow4" "pdfs_slow5")

total_pass=0
total_fail=0
total_timeout=0
total_error=0

for dataset in "${DATASETS[@]}"; do
    dir="$TEST_DIR/$dataset"
    if [ ! -d "$dir" ]; then
        echo "SKIP: $dir (not found)"
        continue
    fi

    pass=0
    fail=0
    timeout_count=0
    error_count=0

    echo ""
    echo "=== Dataset: $dataset ==="

    for pdf in "$dir"/*.pdf; do
        [ -f "$pdf" ] || continue
        fname=$(basename "$pdf")

        start_time=$(date +%s%N)
        if timeout "$TIMEOUT" "$BINARY" "$pdf" > /dev/null 2>/tmp/bench_err.txt; then
            end_time=$(date +%s%N)
            elapsed=$(( (end_time - start_time) / 1000000 ))
            if [ "$elapsed" -gt 10000 ]; then
                echo "  SLOW ${elapsed}ms  $fname"
            fi
            pass=$((pass + 1))
        else
            exit_code=$?
            end_time=$(date +%s%N)
            elapsed=$(( (end_time - start_time) / 1000000 ))
            if [ "$exit_code" -eq 124 ]; then
                echo "  TIMEOUT (>${TIMEOUT}s)  $fname"
                timeout_count=$((timeout_count + 1))
            else
                err_msg=$(head -1 /tmp/bench_err.txt 2>/dev/null || echo "unknown")
                echo "  FAIL ${elapsed}ms  $fname  ($err_msg)"
                error_count=$((error_count + 1))
            fi
            fail=$((fail + 1))
        fi
    done

    echo "  --- $dataset: $pass pass, $timeout_count timeout, $error_count error ---"
    total_pass=$((total_pass + pass))
    total_fail=$((total_fail + fail))
    total_timeout=$((total_timeout + timeout_count))
    total_error=$((total_error + error_count))
done

echo ""
echo "============================================"
echo "TOTAL: $total_pass pass, $total_timeout timeout, $total_error error (of $((total_pass + total_fail)))"
echo "============================================"
