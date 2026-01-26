# Building

The slang shader language compiler, `slangc`, is required. You must either set it as the environment variable `SLANGC` or add it to your shell `PATH`. The latter should already be true if you've installed Slang using any traditional means, like an installer or a package manager.

Shader source code is placed in `assets/shaders/slang`, and any shader file with a valid entrypoint is automatically compiled to the target shader language (SPIR-V), and placed in `assets/shaders/target`. Compilation happens at build time, and the build will fail if there are any compile errors.

Shader compile errors may be too long to be printed to stdout, so they are placed in `shader_compile_errors/latest.log`.