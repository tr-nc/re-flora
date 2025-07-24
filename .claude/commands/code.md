# Claude Command: Code

This command helps you write and verify new Rust code based on user requirements.

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
    * **Command:** You must execute `cargo fmt`.

2. **Check:** Analyze the entire project for compile-time errors and warnings.
    * **Command:** You must execute `cargo check`.

3. **Run:** Test the main executable for runtime failures.
    * **Command:** You must execute `cargo run` without a timeout.

4. **Correct:** If the `fmt`, `check`, or `run` command fails, you must automatically correct the code that caused the failure and **restart the entire verification loop from Step 1 (Format)**.
