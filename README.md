# Workspace structure

This project is split into multiple crates in a workspace. The main workspace, `marigold`, contains the main app executable code. All assets and shaders are placed within the marigold workspace member, for simpler asset finding code.

Self-contained subsystems are split into their own workspace members, such as `bvh`. This is useful for certain math/logic-heavy crates which benefit from release mode optimizations; this way, the most performance-critical parts of marigold can be built in release mode without slowing down the entire build process.

# Building with cargo

To build and run the main executable, use `cargo run -p marigold`.

The slang shader language compiler, `slangc`, is required. You must either set it as the environment variable `SLANGC` or add it to your shell `PATH`. The latter should already be true if you've installed Slang using any traditional means, like an installer or a package manager.

Shader source code is placed in `assets/shaders/slang`, and any shader file with a valid entrypoint is automatically compiled to the target shader language (SPIR-V), and placed in `assets/shaders/target` at the same relative path and with the extension removed. Compilation happens at build time, and the build will fail if there are any compile errors.

Shader compile errors may be too long to be printed to stdout, so they are placed in `shader_compile_errors/latest.log`. Previous instances of this file are renamed to reflect their timestamp.

# Building with make

This is useful for preparing a binary for programs like RenderDoc and NVIDIA NSight, or for bundling the program into a distributable archive. After the build, the binary `target/<profile>/marigold` is copied to `out/marigold/marigold`.

- `make debug`: builds the program in debug mode
- `make release`: builds the program in release mode
- `make nsight`: builds the program in debug mode, providing configuration required for nsight debugging
  - enables shader debug info
  - disables validation layers
- `make bundle`: performs `make release`, then archives `out/marigold` into `out/marigold.tar.gz`.
- `make clean`: delete the entire `out` directory
- `make scrub`: deletes non-build related directories that may have accumulated debug info over time
  - `marigold/bvh_debug` and `marigold/shader_compile_errors`

# Environment Variables

## when building

- `SLANGC`: path to `slangc` executable, if not present, assumed to be in the shell `PATH`
- `SHADER_DEBUG_INFO`: set to any value other than 0 to enable shader debug info as a flag passed to the slang compiler

## when running

- `WINIT_UNIX_BACKEND`: if set to `x11`, creates an X11 window; if set to `wayland`, creates a Wayland window; otherwise let winit decide. This is useful for programs like RenderDoc which don't work well in wayland. Has no effect outside of Linux.
- `DISABLE_VALIDATION_LAYERS`: only applies in debug builds; set to any value other than 0 to keep wgpu debug info present but explicitly disable vulkan validation layers
- `PROFILING_INFO`: set to any value other than 0 to log and/or write profiling information to disk for optimization analysis over several builds
