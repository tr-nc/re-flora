# Claude Command: Check

This command helps you check and correct Rust code within this specific environment.

## Usage

To ask Claude to perform checks on the code, just type:

```shell
/check
```

## What This Command Does

The command executes the following steps in order. If any step fails, you must correct the code and restart the entire sequence from Step 1.

1. **Check:** Analyze the project for errors and warnings.
    * **Command:** You must execute `cargo check`.

2. **Run:** Test the executable for runtime failures with a 60-second timeout.
    * **Command:** You must execute `cargo run` without a timeout.

3. **Correct:** If any mistakes are found during the `check` or `run` steps, automatically correct the code. After correction, you must return to Step 1.

4. **Format:** Before finishing, run the formatter on any file that you changed during the correction stage.
    * **Command:** You must execute `cargo fmt`.
