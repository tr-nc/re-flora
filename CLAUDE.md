# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Re: Flora** is an experimental voxel-based relaxation game built with Rust and Vulkan. It's a meditative gardening experience where players design and nurture their own island paradise. The project uses real-time ray tracing for rendering and has a focus on creating a tranquil, creativity-focused gameplay experience.

## Build Commands

### Basic Build & Run
```bash
# Debug build and run
cargo run

# Release build with validation layers disabled
cargo run --release --features "no_validation_layer"
```

### Linux Dependencies (for cargo check only)
```bash
curl https://sh.rustup.rs -sSf | sh
sudo apt install build-essential pkg-config libasound2-dev cmake ninja-build
```

### Available VS Code Commands
From `.vscode/settings.json`, these commands are available via the command-runner extension:
- `cargo run debug` - Debug build
- `cargo run release` - Release build with no validation layers
- `create test env` - Create conda test environment
- `install test env` - Install test environment dependencies
- `run test env` - Run test environment
- `repomix` - Generate compressed repository summary
- `aider` - Launch AI coding assistant

## Code Architecture

### Core System Architecture

The project is structured around several key systems:

1. **Vulkan Wrapper (`src/vkn/`)**: Custom Vulkan abstraction layer handling:
   - Context management (instance, device, surface)
   - Memory management (buffers, textures, allocator)
   - Command buffer and synchronization primitives
   - Shader compilation and pipeline management
   - RTX acceleration structures for ray tracing

2. **Renderer Systems**:
   - **Tracer (`src/tracer/`)**: Core ray tracing renderer with denoising
   - **Builder (`src/builder/`)**: Voxel world generation and scene acceleration structures
   - **EGui Renderer (`src/egui_renderer/`)**: UI rendering integration

3. **Gameplay Systems**:
   - **Camera (`src/gameplay/camera/`)**: First-person camera with movement, audio, and collision
   - **Audio (`src/audio/`)**: 3D audio engine with clip caching and spatial audio
   - **Procedural Placer (`src/procedual_placer/`)**: Procedural content placement system

4. **Utilities (`src/util/`)**: Common utilities including:
   - Buffer and atlas allocation strategies
   - Shader compilation tools
   - Timing and benchmarking utilities

### Shader System

Shaders are organized in `shader/` directory:
- **Compute Shaders**: Builder systems, denoising, ray tracing
- **Graphics Shaders**: Foliage rendering, UI, shadow mapping
- **Include System**: Shared GLSL code in `shader/include/`

The shader compiler (`src/util/compiler.rs`) provides:
- Runtime GLSL to SPIR-V compilation
- Custom include resolution for modular shaders
- Vulkan 1.3 and SPIR-V 1.6 targeting

### Key Technical Details

- **Ray Tracing**: Uses Vulkan RTX with acceleration structures for voxel rendering
- **Denoising**: Spatial and temporal denoising for real-time ray tracing
- **Voxel System**: Contree-based voxel representation for efficient storage
- **Audio**: 3D positional audio with occlusion and environmental effects
- **Memory Management**: Custom allocators for GPU buffers and texture atlases

## Development Setup

### Required Extensions (VS Code)
- `rust-analyzer`: Rust language support
- `shader-lint`: GLSL/HLSL shader linting (not `glslx` for Vulkan shaders)

### Code Style
- **Rust**: Uses `rustfmt.toml` with 100 character line limit
- **Shaders**: Uses `.clang-format` with LLVM style, 4-space indentation, 100 character limit

### Project Structure Notes
- `src/main.rs`: Application entry point
- `src/app/`: Main application controller and state management
- `src/vkn/`: Vulkan wrapper - comprehensive abstraction over Vulkan API
- `shader/`: All GLSL shaders organized by functionality
- `texture/`: Texture assets and documentation
- `assets/`: Game assets including audio files

## Testing Environment
The project includes a Python test environment in `misc/test_env/` for external testing tools and validation.

## Commit Message Conventions

Based on the project's commit history, follow these naming patterns when creating commit messages:

### Format: `<type>: <description>`

**Common Types:**
- `feat:` - New features or functionality
- `fix:` - Bug fixes and corrections
- `refactor:` - Code restructuring without changing functionality
- `docs:` - Documentation updates
- `minor:` - Small changes, updates, or tweaks

**Style Guidelines:**
- Use lowercase for the type prefix
- Write descriptive, action-oriented descriptions
- Be specific about what was changed and why
- For complex changes, include multiple affected areas

**Examples:**
```
feat: implement frustum culling for grass chunk rendering to improve performance
fix: correct chunk AABB calculation for accurate frustum culling
refactor: streamline camera update logic and improve collision handling
docs: update CLAUDE.md with commit naming conventions
minor: adjust grass sway margin constant for better visual quality
```

**Multi-component changes:**
When changes affect multiple systems, list them clearly:
```
feat: enhance player collision detection by introducing ring-based distances and updating camera logic to utilize these distances for improved movement handling
```

**Avoid:**
- Generic messages like "update", "change", or "fix bug"
- Single letter commits (like "t")
- Overly verbose descriptions without clear purpose

## Important Notes
- Shader compilation happens at runtime using the `shaderc` crate
- The project targets Vulkan 1.3 with RTX ray tracing extensions
- Audio system uses the `kira` crate for 3D spatial audio
- Memory allocation uses `gpu-allocator` for efficient GPU memory management
- The build system includes environment variable injection for asset paths