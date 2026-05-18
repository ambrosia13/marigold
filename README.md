# Workspace structure

This project is split into multiple crates in a workspace. The main workspace, `marigold`, contains the main app executable code. All assets and shaders are placed within the marigold workspace member, for simpler asset finding code.

Self-contained subsystems are split into their own workspace members, such as `bvh`. This is useful for certain math/logic-heavy crates which benefit from release mode optimizations; this way, the most performance-critical parts of marigold can be built in release mode without slowing down the entire build process.

There's also separate binary workspaces for testing and profiling, such as `bvh_sample_collector`, which collects data separately for use with bvh profiling.

A list of all the workspaces and their purpose:
- marigold: main app
- bvh_sample_collector: binary program to more easily collect bvh data
- bvh: the bounding volume hierarchy implementation, meant to be compiled in release mode for much faster builds
- mesh_interface: shared interface for meshes in marigold, needed to support multiple mesh-loading binary crates
- gltf_loading: shared library for loading gltf scenes into meshes as defined by the mesh interface

# Profiling and testing

Profiling in marigold is still a work-in-progress. For now, detailed statistics for the construction of bounding volume hierarchies is optionally logged and written to disk.

To use this functionality in marigold, use the environment variable `PROFILING_INFO`. For example, to collect BVH statistics when running a dev build, use this command:
```
PROFILING_INFO=1 cargo run -p marigold
```
Note that collecting profiling information incurs a little extra cost for measuring all the statistics. If this environment variable is set to 0, or not used, this expensive information is not collected, minimal logging is done, and nothing is written to disk. 

For more streamlined, heavyweight data collection, use the crate `bvh_sample_collector`. It takes some command line arguments:

1. mesh directory: parent directory in which to look for gltf scene directories
2. snapshot directory: parent directory in which to save data
3. generation name: the name of the directory inside the snapshot directory to save all bvh data. For example, if you're testing out a cool optimization to the BVH and you want the stats files to be placed in `bvh_snapshots/cool_optimization/*.json`, the snapshot directory would be `bvh_snapshots` and the generation name would be `cool_optimization`.
4. number of samples: an integral value of the number of times to build a bvh for a single mesh, more samples means less noise in non-deterministic stats like construction time

I recommend running this crate in release mode, since debug mode is not as reliable for profiling. Here is an example command:
```
cargo run --release -p bvh_sample_collector -- meshes bvh_snapshots baseline 15
```

Note that the `PROFILING_INFO` environment variable is not used for this crate.

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

# Environment variables

## when building

- `SLANGC`: path to `slangc` executable, if not present, assumed to be in the shell `PATH`
- `SHADER_DEBUG_INFO`: set to any value other than 0 to enable shader debug info as a flag passed to the slang compiler

## when running

- `WINIT_UNIX_BACKEND`: if set to `x11`, creates an X11 window; if set to `wayland`, creates a Wayland window; otherwise let winit decide. This is useful for programs like RenderDoc which don't work well in wayland. Has no effect outside of Linux.
- `DISABLE_VALIDATION_LAYERS`: only applies in debug builds; set to any value other than 0 to keep wgpu debug info present but explicitly disable vulkan validation layers
- `PROFILING_INFO`: set to any value other than 0 to log and/or write profiling information to disk for optimization analysis over several builds
- `SINGLE_THREADED`: make bevy ECS use a single threaded system executor. this doesn't make the program as a whole run as a single thread, just the ECS
