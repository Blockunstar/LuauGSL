# LuauGSL — Luau GPU Shading Language

A typed shading language compiling to SPIR-V, based on Luau syntax. Designed for GPU compute and rendering pipelines with a zero-cost security model, embeddable runtime, and clean Lua-inspired syntax.

## Architecture

The compiler is organized as a Rust workspace with a multi-phase pipeline:

```
Source (.gsl) -> Lexer -> Parser -> AST -> Type Checker -> HIR -> Optimizer -> SPIR-V Codegen -> .spv
```

### Workspace Crates

| Crate | Purpose |
|-------|---------|
| `luaugsl-core` | Type system (`Type` enum), AST nodes, error types, standard library definitions, shader metadata |
| `luaugsl-lexer` | Hand-written recursive descent lexer producing a stream of `Token` values |
| `luaugsl-parser` | Recursive descent parser building an AST (`Module`, `FunctionDecl`, `Expr`, `Stmt`) |
| `luaugsl-typeck` | Type inference and checking with capability enforcement per shader stage |
| `luaugsl-hir` | High-level IR: AST lowering to CFG-friendly HIR with basic block structure |
| `luaugsl-opt` | Optimization passes: constant folding, dead code elimination, algebraic simplification |
| `luaugsl-spirv` | SPIR-V code generation via `rspirv` with full GLSL.std.450 extended instruction support |
| `luaugsl-cli` | Command-line interface: `compile`, `verify`, `info`, `test` subcommands |
| `luaugsl-test` | Integration tests covering all 6 example shaders from the specification |

## Building

```bash
cd /path/to/luaugsl
cargo build --release
```

The `luaugsl` binary will be available at `target/release/luaugsl`.

## Usage

### Compile a shader

```bash
luaugsl compile input.gsl -o output.spv
```

### Verify a SPIR-V file

```bash
luaugsl verify output.spv
```

### Print shader reflection info

```bash
luaugsl info output.spv
```

### Run integration tests

```bash
luaugsl test
```

## Example Shaders

Six example shaders are included in `examples/` covering the major features:

| Example | Stage | Features Demonstrated |
|---------|-------|----------------------|
| `basic_fragment.gsl` | Fragment | Texture sampling, PBR lighting, tone mapping, color namespace |
| `vertex_shader.gsl` | Vertex | MVP transforms, vertex attributes, builtins, animation |
| `compute_shader.gsl` | Compute | Storage buffers, workgroup barriers, 1D array processing |
| `instanced_rendering.gsl` | Vertex + Fragment | Type aliases, SSBOs, instancing, multi-function shader |
| `post_process.gsl` | PostProcess | Storage images, imageRead/imageWrite, nested loops, 2D convolution |
| `raymarch_sdf.gsl` | PostProcess | User-defined functions, SSBO iteration, smooth blending, raymarching |

## Language Features

### Type System
- Primitive types: `nil`, `boolean`, `f32`, `i32`, `u32`
- Vector types: `vector2/3/4`, `vector2/3/4i`, `vector2/3/4u`, `bvector2/3/4`
- Matrix types: `mat2x2`, `mat3x3`, `mat4x4`
- GPU resources: `texture2d`, `texture3d`, `textureCube`, `storageBuffer`, `storageImage`, `sampler`
- Type aliases via `type Name = { ... }`
- Generic types with `<T>` syntax

### Shader Stages
- `@stage("vertex")` — Vertex shaders with `@builtin` and `@location` I/O
- `@stage("fragment")` — Fragment shaders with `discard`, derivatives
- `@stage("compute")` — Compute shaders with `@workgroupSize`, barriers
- `@stage("postprocess")` — Fullscreen compute passes

### Standard Library
- **Math**: `sin`, `cos`, `sqrt`, `pow`, `clamp`, `mix`, `smoothstep`, etc.
- **Vector/Matrix**: constructors, swizzling, `dot`, `cross`, `normalize`, `inverse`, `transpose`
- **Texture**: `sample`, `sampleLod`, `textureFetch`, `imageRead`, `imageWrite`
- **Buffer**: `bufferLoad`, `bufferStore`, `bufferLength`, atomic operations
- **Noise**: `noise.perlin`, `noise.simplex`, `noise.fbm`, `noise.turbulence`, `noise.value`
- **Color**: `color.ACESFilm`, `color.linearTosRGB`, `color.reinhard`, `color.luminance`
- **Geometry**: `geo.raySphere`, `geo.rayAABB`, `geo.barycentric`, `geo.sampleHemisphereCosine`
- **Bit32**: `bit32.band`, `bit32.lshift`, `bit32.extract`, `bit32.packHalf2x16`
- **Subgroup** (compute): `subgroupAdd`, `subgroupBroadcast`, `subgroupBallot`

### Security
- DoS prevention via compile-time safety limits on loop nesting, instruction count, call depth
- Stage-appropriate capability enforcement
- No dynamic recursion (explicit graph check)

## Specification

The full language specification is available in `docs/LuauGSL_Specification_v1.1.2.pdf`, covering:
- Lexical analysis (tokens, literals, comments, attributes)
- Grammar (expressions, statements, declarations)
- Type system (inference, subtyping, casts)
- Shader stages and I/O attributes
- Standard library reference
- SPIR-V mapping and GLSL.std.450 encoding
- Security model and safety limits
- Binary artifact format (LGSA)
- Runtime API design

## License

MIT OR Apache-2.0
