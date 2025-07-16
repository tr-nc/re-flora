# Claude Command: Code

This command helps you write code and perform checks & formatting.

## Usage

To ask claude to write code, just type:

```shell
/code
```

## What This Command Does

1. Writes code for you with your repuirements
2. If given with -u, claude will do ultrathink for task planning stages, and then think with normal effort for code writing to balance the effort and the quality.
3. After modifying shaders, claude will take extra caution on the changing of signature, to prevent other shader files from failing to compile.
4. After code is written, claude will preform cargo check and cargo fmt.

## Example

```shell
/code # For normal code writing
/code -u # For complex code writing and planning
```
