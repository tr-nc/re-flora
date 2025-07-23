# Claude Command: Check

This command helps you check and correct Rust code within this specific environment.

> **⭐ Golden Rule: You MUST Use the Full Path ⭐**
>
> For all `cargo` operations, you are required to use the full, absolute path to the executable. Do not use the simple `cargo` command.
>
> **Correct Path Variable:**
> `CARGO_PATH = /mnt/c/Users/danny/.cargo/bin/cargo.exe`
>
> Always use this exact path.

## Usage

To ask Claude to perform checks on the code, just type:

```shell
/check
```

## What This Command Does

The command executes the following steps in order. If any step fails, you must correct the code and restart the entire sequence from Step 1.

1. **Check:** Analyze the project for errors and warnings.
    * **Command:** You must execute `{CARGO_PATH} check`.

2. **Run:** Test the executable for runtime failures with a 60-second timeout.
    * **Command:** You must execute `timeout 60 {CARGO_PATH} run`.

3. **Correct:** If any mistakes are found during the `check` or `run` steps, automatically correct the code. After correction, you must return to Step 1.

4. **Format:** Before finishing, run the formatter on any file that you changed during the correction stage.
    * **Command:** You must execute `{CARGO_PATH} fmt`.

## Constraints and Rules

* **ABSOLUTELY NO** use of the shorthand `cargo` command is permitted. Always use the full path: `/mnt/c/Users/danny/.cargo/bin/cargo.exe`.
* Do not assume `cargo` is in the system's `PATH`.
* Every command involving the Rust toolchain must explicitly use the full path as defined in the Golden Rule.

### Reasoning

It is critical to use the full path to the `cargo.exe` executable to prevent ambiguity. This ensures that we are using the correct version of the Rust toolchain installed within the Windows Subsystem for Linux (WSL) environment and avoids conflicts with other potentially installed versions.
