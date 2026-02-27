@echo off
REM Performance Benchmark Runner for Sandbox Security Solution
REM 
REM This script runs the performance benchmarks and generates reports
REM Requirements: Non-functional requirement 4.1

echo ========================================
echo Sandbox Performance Benchmarks
echo ========================================
echo.

REM Check if cargo is available
where cargo >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: cargo not found. Please install Rust.
    exit /b 1
)

echo Running benchmarks...
echo This may take several minutes...
echo.

REM Run benchmarks with criterion
cargo bench --bench sandbox_benchmarks

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo ERROR: Benchmarks failed to run
    exit /b 1
)

echo.
echo ========================================
echo Benchmark Results
echo ========================================
echo.
echo Results have been saved to: target\criterion
echo.
echo To view the HTML report, open:
echo   target\criterion\report\index.html
echo.
echo Performance Targets (Non-functional requirement 4.1):
echo   - Application startup time increase: ^<2 seconds
echo   - Tool execution delay: ^<100ms
echo   - Memory overhead: ^<10%% of host system
echo.

pause
