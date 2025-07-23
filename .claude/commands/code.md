# Claude Command: Code

This command helps you write and verify new Rust code based on user requirements.

> **⭐ Golden Rule: You MUST Use the Full Path ⭐**
>
> For all `cargo` operations, you are required to use the full, absolute path to the executable. Do not use the simple `cargo` command.
>
> **Correct Path Variable:**
> `CARGO_PATH = /mnt/c/Users/danny/.cargo/bin/cargo.exe`
>
> Always use this exact path for all `fmt`, `check`, and `run` operations.

## Usage

To ask Claude to write code, just type:

```shell
/code [your requirements here]
```

## What This Command Does

The command follows a strict two-phase process: **Code Generation** followed by a **Verification Loop**.

### Phase 1: Code Generation

1. **Write Code:** Generate the necessary Rust code based on the user's requirements.
2. **Handle Shaders with Caution:** When modifying any shader files, you must be extremely careful with function signatures to prevent compilation failures in other dependent shader files.

### Phase 2: Verification and Correction Loop

After generating the initial code, you **must** enter the following loop. This loop repeats until all three steps succeed in order.

1. **Format:** Automatically format all files you have created or changed.
    * **Command:** You must execute `{CARGO_PATH} fmt`.

2. **Check:** Analyze the entire project for compile-time errors and warnings.
    * **Command:** You must execute `{CARGO_PATH} check`.

3. **Run:** Test the main executable for runtime failures with a 60-second timeout.
    * **Command:** You must execute `timeout 60 {CARGO_PATH} run`.

4. **Correct:** If the `fmt`, `check`, or `run` command fails, you must automatically correct the code that caused the failure and **restart the entire verification loop from Step 1 (Format)**.

## Constraints and Rules

* **ABSOLUTELY NO** use of the shorthand `cargo` command is permitted. Always use the full path: `/mnt/c/Users/danny/.cargo/bin/cargo.exe`.
* Do not assume `cargo` is in the system's `PATH`.
* Every command involving the Rust toolchain must explicitly use the `{CARGO_PATH}` variable as defined in the Golden Rule.

### Reasoning

It is critical to use the full path to the `cargo.exe` executable to prevent ambiguity. This ensures that we are using the correct version of the Rust toolchain installed within the Windows Subsystem for Linux (WSL) environment and avoids conflicts with other potentially installed versions.
