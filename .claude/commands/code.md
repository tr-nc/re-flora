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

1. Writes code for you with your repuirements. If given with --ultra-think, claude will use ultrathink.
2. When shaders are modified, claude will take extra caution on the changing of signature, to prevent other shader files from failing to compile.
3. After all writing is done, Claude will execute `WINDOWS_CARGO_PATH fmt` on any file that Claude have changed during the code writing stage to ensure the code style is consistent.
4. Claude will then perform a /check command to ensure the code is correct. (See ./check.md for more details.)

## Example

```shell
/code # For code writing
/code --ultra-think # For code writing with ultrathink planning
```
