@echo off
setlocal enabledelayedexpansion

rem ============================================================================
rem Clang-Format Batch Script
rem
rem This script recursively finds and formats files with specific extensions
rem in the current directory and all subdirectories. It uses clang-format
rem and respects the settings in your .clang-format file.
rem
rem Prerequisites:
rem 1. clang-format must be installed and accessible in your system's PATH.
rem 2. A .clang-format file should be present in the root directory where you
rem    run this script, or in any parent directory.
rem
rem Configuration:
rem - Edit the "FILE_EXTENSIONS" variable below to change or add file types.
rem   Separate extensions with a space (e.g., "*.glsl *.hlsl").
rem ============================================================================

echo.
echo Starting formatting rust code
cargo fmt

rem --- CONFIGURATION ---
rem Define the file extensions to format. Use a space-separated list.
rem You can add more extensions here, like *.glsl
set "FILE_EXTENSIONS=*.vert *.frag *.comp *.glsl"

rem --- SCRIPT LOGIC ---
echo.
echo Starting formatting shaders
echo Searching for files with extensions: %FILE_EXTENSIONS%
echo.

rem Check if clang-format is available
where clang-format >nul 2>nul
if %errorlevel% neq 0 (
    echo ERROR: clang-format not found in your system's PATH.
    echo Please install LLVM and ensure clang-format is in your PATH.
    goto :eof
)

rem --- NEW: First pass to count the total number of files ---
echo Counting files to format...
set total_files=0
for /R . %%F in (%FILE_EXTENSIONS%) do (
    set /a total_files+=1
)
echo Found !total_files! files.

rem --- MODIFIED: Main loop to find and format files with progress ---
set current_file=0
for /R . %%F in (%FILE_EXTENSIONS%) do (
    rem Increment the counter for the current file
    set /a current_file+=1
    
    rem Display progress like "1/23 done"
    rem We use !variable! syntax because delayed expansion is enabled.
    echo !current_file!/!total_files! done
    
    rem Execute clang-format on the file.
    rem -i: Modifies the file in-place.
    rem --style=file: Tells clang-format to look for a .clang-format file.
    rem 2>nul: Suppresses benign "The system cannot find the drive specified" messages.
    clang-format -i --style=file "%%F" 2>nul
)

echo.
echo =================================
echo Formatting process complete.
echo =================================
echo.

endlocal