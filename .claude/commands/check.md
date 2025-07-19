# Claude Command: Check

This command helps you check, and correct the code.

## Usage

To ask Claude to perform checks on the code, just type:

```shell
/check
```

## Configurables

WINDOWS_CARGO_PATH: /mnt/c/Users/danny/.cargo/bin/cargo.exe

## What This Command Does

1. **Check:** Performs `WINDOWS_CARGO_PATH check` to analyze the project for errors and warnings.

2. **Run:** Claude will run the executable using `timeout 10 WINDOWS_CARGO_PATH run` to test for runtime failures.

3. **Correct:** If any mistakes are found during the `check` or `run` steps, Claude will automatically correct the code and repeat the process until it succeeds.

4. **Format:** Executes `WINDOWS_CARGO_PATH fmt` on any file that Claude have changed during the correction stage to ensure the code style is consistent.
