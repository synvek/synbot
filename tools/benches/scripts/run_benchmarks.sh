#!/bin/bash
# Performance Benchmark Runner for Sandbox Security Solution
# 
# This script runs the performance benchmarks and generates reports
# Requirements: Non-functional requirement 4.1

set -e

echo "========================================"
echo "Sandbox Performance Benchmarks"
echo "========================================"
echo ""

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo "ERROR: cargo not found. Please install Rust."
    exit 1
fi

echo "Running benchmarks..."
echo "This may take several minutes..."
echo ""

# Run benchmarks with criterion
cargo bench --bench sandbox_benchmarks

echo ""
echo "========================================"
echo "Benchmark Results"
echo "========================================"
echo ""
echo "Results have been saved to: target/criterion"
echo ""
echo "To view the HTML report, open:"
echo "  target/criterion/report/index.html"
echo ""
echo "Performance Targets (Non-functional requirement 4.1):"
echo "  - Application startup time increase: <2 seconds"
echo "  - Tool execution delay: <100ms"
echo "  - Memory overhead: <10% of host system"
echo ""
