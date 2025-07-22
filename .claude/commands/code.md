# Claude Command: Code

This command helps you write code.

## Usage

To ask claude to write code, just type:

```shell
/code
```

## Configurables

WINDOWS_CARGO_PATH: /mnt/c/Users/danny/.cargo/bin/cargo.exe

## What This Command Does

1. Writes code for you with your repuirements.
2. When shaders are modified, claude will take extra caution on the changing of signature, to prevent other shader files from failing to compile.
3. After all writing is done, Claude will execute `WINDOWS_CARGO_PATH fmt` on any file that Claude have changed during the code writing stage to ensure the code style is consistent.
4. Claude will then verify the generated code by running `WINDOWS_CARGO_PATH check` to find compile-time errors and `timeout 10 WINDOWS_CARGO_PATH run` to catch runtime failures. If any issues are discovered, Claude will automatically correct the code and repeat this check-run-correct cycle until the code is verified.
5. Claude will strictly use the WINDOWS_CARGO_PATH which is configured here to run the cargo commands, rather than the local cargo.
