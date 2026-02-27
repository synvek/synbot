#!/usr/bin/env python3
"""
Benchmark Analysis Tool for Sandbox Security Solution

This script analyzes benchmark results and checks against performance requirements.
Requirements: Non-functional requirement 4.1

Performance Targets:
- Application startup time increase: <2 seconds
- Tool execution delay: <100ms  
- Memory overhead: <10% of host system
"""

import json
import sys
import os
from pathlib import Path
from typing import Dict, List, Tuple

# Performance thresholds (from requirements)
THRESHOLDS = {
    'app_startup_time_ms': 2000,  # 2 seconds
    'tool_execution_latency_ms': 100,  # 100ms
    'memory_overhead_percent': 10,  # 10%
}

def load_benchmark_results(criterion_dir: Path) -> Dict:
    """Load benchmark results from criterion output directory"""
    results = {}
    
    # Criterion stores results in subdirectories
    for bench_dir in criterion_dir.iterdir():
        if not bench_dir.is_dir():
            continue
            
        # Look for estimates.json
        estimates_file = bench_dir / "base" / "estimates.json"
        if estimates_file.exists():
            with open(estimates_file, 'r') as f:
                data = json.load(f)
                results[bench_dir.name] = data
    
    return results

def analyze_startup_time(results: Dict) -> Tuple[bool, str]:
    """Analyze application startup time"""
    startup_benchmarks = [k for k in results.keys() if 'app_sandbox_startup' in k]
    
    if not startup_benchmarks:
        return False, "No startup benchmarks found"
    
    messages = []
    all_pass = True
    
    for bench_name in startup_benchmarks:
        data = results[bench_name]
        # Criterion stores time in nanoseconds
        mean_time_ns = data.get('mean', {}).get('point_estimate', 0)
        mean_time_ms = mean_time_ns / 1_000_000
        
        threshold = THRESHOLDS['app_startup_time_ms']
        passed = mean_time_ms < threshold
        all_pass = all_pass and passed
        
        status = "✓ PASS" if passed else "✗ FAIL"
        messages.append(
            f"  {status} {bench_name}: {mean_time_ms:.2f}ms "
            f"(threshold: {threshold}ms)"
        )
    
    return all_pass, "\n".join(messages)

def analyze_execution_latency(results: Dict) -> Tuple[bool, str]:
    """Analyze tool execution latency"""
    latency_benchmarks = [k for k in results.keys() if 'tool_execution_latency' in k]
    
    if not latency_benchmarks:
        return False, "No execution latency benchmarks found"
    
    messages = []
    all_pass = True
    
    for bench_name in latency_benchmarks:
        data = results[bench_name]
        mean_time_ns = data.get('mean', {}).get('point_estimate', 0)
        mean_time_ms = mean_time_ns / 1_000_000
        
        threshold = THRESHOLDS['tool_execution_latency_ms']
        passed = mean_time_ms < threshold
        all_pass = all_pass and passed
        
        status = "✓ PASS" if passed else "✗ FAIL"
        messages.append(
            f"  {status} {bench_name}: {mean_time_ms:.2f}ms "
            f"(threshold: {threshold}ms)"
        )
    
    return all_pass, "\n".join(messages)

def analyze_memory_overhead(results: Dict) -> Tuple[bool, str]:
    """Analyze memory overhead"""
    memory_benchmarks = [k for k in results.keys() if 'memory_overhead' in k]
    
    if not memory_benchmarks:
        return False, "No memory overhead benchmarks found"
    
    messages = []
    messages.append("  Note: Memory overhead analysis requires manual measurement")
    messages.append("  Use system monitoring tools to measure actual memory usage")
    messages.append(f"  Target: <{THRESHOLDS['memory_overhead_percent']}% of host system")
    
    return True, "\n".join(messages)

def analyze_concurrent_performance(results: Dict) -> Tuple[bool, str]:
    """Analyze concurrent operation performance"""
    concurrent_benchmarks = [k for k in results.keys() if 'concurrent_operations' in k]
    
    if not concurrent_benchmarks:
        return False, "No concurrent operation benchmarks found"
    
    messages = []
    
    for bench_name in concurrent_benchmarks:
        data = results[bench_name]
        mean_time_ns = data.get('mean', {}).get('point_estimate', 0)
        mean_time_ms = mean_time_ns / 1_000_000
        
        messages.append(f"  {bench_name}: {mean_time_ms:.2f}ms")
    
    return True, "\n".join(messages)

def main():
    """Main analysis function"""
    print("=" * 60)
    print("Sandbox Performance Benchmark Analysis")
    print("=" * 60)
    print()
    
    # Find criterion results directory
    criterion_dir = Path("target") / "criterion"
    
    if not criterion_dir.exists():
        print("ERROR: Benchmark results not found.")
        print("Please run benchmarks first: cargo bench --bench sandbox_benchmarks")
        sys.exit(1)
    
    # Load results
    print("Loading benchmark results...")
    results = load_benchmark_results(criterion_dir)
    
    if not results:
        print("ERROR: No benchmark results found in", criterion_dir)
        sys.exit(1)
    
    print(f"Found {len(results)} benchmark results")
    print()
    
    # Analyze each category
    all_passed = True
    
    print("1. Application Startup Time")
    print("-" * 60)
    passed, message = analyze_startup_time(results)
    print(message)
    print()
    all_passed = all_passed and passed
    
    print("2. Tool Execution Latency")
    print("-" * 60)
    passed, message = analyze_execution_latency(results)
    print(message)
    print()
    all_passed = all_passed and passed
    
    print("3. Memory Overhead")
    print("-" * 60)
    passed, message = analyze_memory_overhead(results)
    print(message)
    print()
    
    print("4. Concurrent Operations")
    print("-" * 60)
    passed, message = analyze_concurrent_performance(results)
    print(message)
    print()
    
    # Summary
    print("=" * 60)
    print("Summary")
    print("=" * 60)
    
    if all_passed:
        print("✓ All performance requirements met!")
        print()
        print("Performance targets (Non-functional requirement 4.1):")
        print(f"  ✓ Application startup time: <{THRESHOLDS['app_startup_time_ms']}ms")
        print(f"  ✓ Tool execution latency: <{THRESHOLDS['tool_execution_latency_ms']}ms")
        print(f"  • Memory overhead: <{THRESHOLDS['memory_overhead_percent']}% (manual check)")
        sys.exit(0)
    else:
        print("✗ Some performance requirements not met")
        print()
        print("Please review the results above and optimize accordingly.")
        sys.exit(1)

if __name__ == "__main__":
    main()
