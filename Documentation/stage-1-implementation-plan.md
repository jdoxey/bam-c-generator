# Stage 1 implementation plan: first working `bam`

## Goal

Implement the first usable `bam` source generator so it can translate both checked-in examples:

- `Documentation/examples/Hexagon/Hexagon.bam`
- `Documentation/examples/HexagonFarm/HexagonFarm.bam`

For each input, generate the corresponding ordinary C header and source file, then compile and run the existing example programs on Linux, macOS, and Windows. Stage 1 is complete when generation is deterministic, generated C matches the documented API, imports between the examples work, and the platform build checks pass. The initial support matrix is x86-64 Linux with a documented glibc baseline (recommended: glibc 2.17), x86-64 and arm64 macOS, and x86-64 Windows; add other architectures only when CI runners and release artifacts are available. Ensure the Linux release build actually meets the chosen libc baseline, rather than assuming a binary linked on the hosted runner will.

## Read these project decisions first

- `Documentation/Features.md` defines the user-visible object, `init`, `new`, `deinit`, and `delete` behavior and shows expected generated output.
- `Documentation/ADR-0001-bam-implementation.md` recommends a Rust CLI, Tree-sitter C grammar, a `bam`-specific front end, an internal representation (IR), and portable generated C.
- The two `.bam` files are the initial language fixtures. The `.c` and `.h` files beside them are reference output, and the test programs and Farm Makefile show intended use.

Do not treat every C feature as supported just because Tree-sitter can parse it. Stage 1 should support the syntax exercised by these inputs and reject other syntax clearly.

**Instruction for builder agents:** Create additional examples to exercise supported features that the existing examples do not cover. If a feature can be added to an existing example with a small, clear change, extend that example instead. Keep the two current examples as required end-to-end fixtures; additional examples should cover meaningful gaps in parsing, generation, lifecycle behavior, imports, and diagnostics without expanding the promised language subset accidentally.

## Stage 1 scope

Implement:

- A native Rust command-line executable named `bam`.
- A `generate` command that accepts a `.bam` input and writes sibling `<Stem>.h` and `<Stem>.c` files. For example, `bam generate Documentation/examples/Hexagon/Hexagon.bam` writes `Hexagon.h` and `Hexagon.c` in that directory.
- `.bam` imports for standard headers and other `.bam` objects.
- C-like field declarations, method declarations/definitions, parameters, and bodies used by the examples.
- Implicit instance receiver generation: ordinary methods receive the object pointer as their first C parameter; method bodies already use `self` and are copied without textual rewriting.
- Generated struct definitions, prototypes, allocation, initialization, cleanup, and method definitions.
- Deterministic output, useful errors with input path and line/column, and no partial output files after a parse/validation failure.
- GitHub Actions CI that builds the Rust CLI and runs generation, C compilation, and example programs on Linux, macOS, and Windows.

Defer arbitrary C preprocessing, compiler extensions, general C type checking, unrelated C syntax, project-wide build orchestration, editor integration, and automatic memory management beyond the explicit `init`/`deinit`/`new`/`delete` conventions.

## Language and output contract to implement

Write down these Stage 1 rules in code comments or a short language reference before they are relied on:

1. **Object naming:** The input stem determines the C type and generated function prefix (`Hexagon.bam` → `Hexagon`). Define identifier validation and header-guard generation. Use portable include guards that do not reserve identifiers (avoid leading underscores followed by uppercase letters).
2. **Fields:** Accept the field declarators in the examples, including scalar `double`, `int`, and `Hexagon **`. Preserve the declaration in the struct. Reject unsupported or ambiguous declarators rather than guessing.
3. **Methods:** Accept a return type, method name, parameter list, and C body. Generated instance methods receive `<Type> *self` first. Preserve explicit parameter names and body text. `init` and `deinit` are reserved lifecycle method names.
4. **Initialization and allocation:** Emit `Type_init(Type *self, ...)` from an explicit `init`. Emit `Type_new(...)` with the same parameters; it allocates, checks allocation, invokes init, and returns the pointer. If there is no init, emit `Type_new(void)` only if zero-initialized state is valid; initialize the allocated struct with `*self = (Type){0}` (C99 compound-literal semantics), rather than assuming `calloc`'s all-bits-zero representation is valid for every C type.
5. **Cleanup and deletion:** Emit `Type_deinit(Type *self)` only when there is an explicit `deinit`. Emit `Type_delete(Type *self)` for heap objects; it calls deinit when present and then frees self. Make delete accept `NULL` as a no-op. `deinit` must not free self, so it is usable for stack storage. Callers must still check the result of `new` before dereferencing it.
6. **Imports:** Support `from <stdlib> import {malloc, free}` and `from "../Hexagon/Hexagon" import *` as used by Farm. Map the standard import to `#include <stdlib.h>` and map the object import to `#include "../Hexagon/Hexagon.h"` in the generated Farm header. Resolve quoted paths relative to the importing `.bam` file and detect missing files and import cycles. Stage 1 implements named standard-header imports and wildcard object imports only; reject unsupported import forms.
7. **Dependency output:** A `generate` invocation writes only the requested object's pair. It does not recursively generate imported objects. Therefore generate Hexagon before HexagonFarm in a clean tree; Farm generation verifies the source import exists and emits an include for the corresponding generated header, which must be available when compiling Farm output.
8. **Generated includes:** Include required C standard headers for generated boilerplate and imported object headers. Avoid emitting duplicate includes. Ensure generated C uses the correct local header path/name.
9. **Preprocessor:** No arbitrary macro expansion is promised. Preserve or reject preprocessor directives explicitly; do not silently parse branches as if macro conditions were evaluated.
10. **Errors:** Report unsupported declarations, duplicate fields/methods, duplicate lifecycle methods, methods that collide with generated names (including `new` and `delete`), unresolved imports, and Tree-sitter error/missing nodes with `.bam` source locations.

**Stage 1 initialization failure contract:** Preserve the documented `void init` API and generate the examples as currently written. `Type_new` detects failure to allocate the object itself, but cannot detect failures from allocations inside an authored `void init`; the Farm example therefore assumes its internal allocations succeed. Document this limitation and do not imply otherwise. A fallible initializer API is deferred; changing to it later requires updating `Features.md`, generated APIs, examples, and callers together. Generated `Type_delete(NULL)` is a no-op. A user-authored `deinit` may assume it receives an initialized object; null checks and partial-init cleanup inside user bodies are the author's responsibility and are not guaranteed by Stage 1.

## Recommended implementation layout

Keep modules small and separated by responsibility. Names can follow the implementer's Rust conventions:

- CLI argument parsing and exit codes.
- `.bam` import/directive recognition.
- Tree-sitter C syntax parsing and source-location conversion.
- Validation and conversion into owned IR types such as `Object`, `Field`, `Method`, `Import`, and source spans.
- C header/source generation from validated IR.
- File output, path resolution, and overwrite/error behavior.

Use Tree-sitter's C grammar for C-like constructs. The `.bam` imports are not C syntax, so identify them in a source-preserving front-end pass: parse each import, replace its bytes with spaces while preserving newlines (or otherwise maintain a source map), and parse the remaining C-like text. Retain the original source buffer and use node byte ranges to preserve method bodies. Never rewrite `self` using global string replacement.

Pin Tree-sitter runtime and grammar versions; include generated parser sources or otherwise ensure installing/running `bam` does not require Node.js, the Tree-sitter CLI, or a C compiler. The generated executable should not invoke a platform shell to parse or generate files.

## Suggested work sequence

### 1. Establish the Rust project and CLI skeleton

- Create the Cargo package, executable target, license metadata for dependencies, and basic help/version behavior.
- Define the command form and stable exit-code convention. Recommended initial form: `bam generate <input.bam>`.
- Make paths platform-neutral with Rust path APIs; test spaces and non-ASCII path components where supported.

### 2. Implement source loading and import discovery

- Read input as UTF-8 and report invalid encoding/read errors cleanly.
- Recognize the two import forms used by Farm without confusing import strings with C string literals.
- Normalize quoted import paths relative to their source file, add the `.bam` extension if that is the documented convention, and resolve imported `.bam` paths.
- Track visited and active import paths to detect cycles. Validate that the direct imported source exists. To detect transitive cycles, recursively scan imported files' import declarations only; do not parse their C declarations or bodies, and do not generate their outputs unless separately requested.
- Map standard imports to C headers and object imports to generated header dependencies.

### 3. Parse and validate the supported subset

- Parse masked import-free text with the C grammar.
- Convert top-level field declarations and method definitions into IR, preserving exact source ranges for bodies and useful ranges for diagnostics.
- Reject syntax errors, missing nodes, unsupported declarators, duplicate names, duplicate lifecycle methods, and unrecognized top-level forms.
- Add direct fixture coverage for `Hexagon.bam` and `HexagonFarm.bam` before implementing code generation.
- Builder agents should add or extend examples to cover supported features not exercised by those fixtures, keeping changes small when an existing example can demonstrate the feature.

### 4. Generate the Hexagon pair

- Generate a portable include guard and struct with `double sideLength`.
- Generate `Hexagon_init`, `Hexagon_new`, `Hexagon_delete`, and `Hexagon_area` signatures/definitions.
- Ensure `new` checks `malloc` before init, and that method body text is preserved.
- Compare against the reference `.h`/`.c` after normalizing only intentionally chosen formatting differences.

### 5. Add dependency and lifecycle generation for HexagonFarm

- Generate the object header dependency `#include "../Hexagon/Hexagon.h"` in `HexagonFarm.h` and the `Hexagon **hexagons` field.
- Map the `stdlib` import to `#include <stdlib.h>` in `HexagonFarm.c`, where the imported functions are used. The generated source's own header provides the imported Hexagon declarations transitively through its field type.
- Generate `HexagonFarm_new`, `HexagonFarm_init`, `HexagonFarm_deinit`, `HexagonFarm_delete`, and `HexagonFarm_totalArea`.
- Ensure `delete` calls `deinit` before freeing the farm; `deinit` body remains user-authored and frees its owned children and array as shown in `.bam`.
- Generate declarations in the header in a stable order and avoid duplicate helper includes.
- Document and use dependency order: generate Hexagon first, then HexagonFarm. Farm generation does not emit Hexagon files.

### 6. Define output safety and repeatability

- Do all parsing, dependency loading, and validation before replacing generated outputs.
- Write outputs to temporary sibling files and rename them only after successful generation, or use another cross-platform strategy that avoids leaving truncated files.
- Define overwrite behavior. Recommended: overwrite the corresponding generated files on success, never overwrite unrelated files, and emit a clear error on filesystem failures.
- Ensure identical input yields byte-identical output across runs and platforms (use `\n` line endings in generated C if feasible).

### 7. Verify both examples in GitHub Actions

- Add parser/IR tests for both fixtures, generated-output golden tests, and negative cases for malformed/unsupported syntax and bad imports.
- Generate Hexagon first and then HexagonFarm into a temporary copy/tree. Treat checked-in `.h`/`.c` files as golden outputs: update them intentionally to the portable include guards and final generator formatting, then compare generated and checked-in files byte-for-byte.
- Add a GitHub Actions workflow triggered by pushes and pull requests. Use native hosted runners for x86-64 Linux (`ubuntu-24.04`), x86-64 macOS (`macos-15-intel`), arm64 macOS (`macos-15`), and x86-64 Windows (`windows-2025`). Runner labels and availability can change; verify the [current GitHub-hosted runner list](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) when implementing the workflow.
- In each job, install/use the pinned Rust toolchain, build `bam`, generate Hexagon first and HexagonFarm second in a clean fixture copy, compile the final generated `.c` files together with their generated `.h` files included by the test programs, run both programs, and check generated outputs against the golden files.
- Update the example test programs to check `new` results before dereferencing them, as required by the generated API.
- Compile the final generated C/header outputs with GCC on Linux, LLVM/Clang on macOS, and a pinned Visual Studio 2022 MSVC toolchain supporting `/std:c11` on Windows. Compile the generated `.c` files with the corresponding example test programs so the generated headers are parsed and validated by each compiler. The existing Farm Makefile is POSIX-oriented, so CI should use platform-specific compiler commands or a cross-platform build script; do not depend on `make` being installed on Windows.
- Keep each workflow step's working directory and paths explicit. Ensure the test exercises both a repository-root invocation and a path with the current working directory outside the repository root.
- Fix any mismatches between `.bam`, reference C, headers, test programs, and build files as part of documenting the expected output; do not silently preserve accidental inconsistencies.
- Test CLI behavior from a working directory other than the repository root, with relative and absolute input paths.

## Acceptance checklist

- [ ] GitHub Actions builds the release `bam` executable and runs the acceptance workflow on the documented Linux, macOS, and Windows targets for pushes and pull requests.
- [ ] One documented command generates Hexagon's `.h` and `.c` from its `.bam` input.
- [ ] After generating Hexagon, generating HexagonFarm resolves its Hexagon import and emits `#include "../Hexagon/Hexagon.h"`; it does not recursively emit the Hexagon pair.
- [ ] The generated public APIs match `Features.md`, including explicit `self`, stack-usable init/deinit, heap new/delete, and NULL checking for the generated allocation.
- [ ] The final generated `.c` and `.h` outputs compile in C11 mode with GCC on Linux, LLVM/Clang on macOS, and MSVC on Windows; both example programs run on each supported platform in the initial GitHub Actions matrix.
- [ ] Re-running generation produces identical files; failed parsing/validation leaves prior outputs intact.
- [ ] Unsupported syntax and import failures return nonzero with a useful `.bam` path and line/column.
- [ ] The executable has no runtime dependency on Node.js, Tree-sitter CLI, or the user's C compiler.
- [ ] The implemented subset and any deferred semantics are documented; no broader C compatibility is implied.

## Known Stage 1 limitations to make explicit

- Tree-sitter provides syntax, not C type checking or macro/preprocessor semantics.
- The generator can preserve C bodies it does not semantically understand; the target C compiler remains responsible for validating those bodies.
- Imported object declarations require the imported object to be generated or its header to be available when compiling the C output.
- `void init` cannot report failure from allocations performed inside an authored init body. Stage 1 explicitly accepts this limitation for the current Farm example; a status-returning init is future work.
- The examples demonstrate a small subset, not full C declarator or compiler-extension support.

## CI reference

- [GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) — runner labels, operating systems, and architectures; check the current list when implementing CI.
