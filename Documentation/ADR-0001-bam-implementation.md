# ADR-0001: Implement `bam` as a Rust source generator with Tree-sitter parsing

- **Status:** Proposed
- **Date:** 2026-11-15

## Context

`bam` takes `.bam` files and generates ordinary C header/source pairs. The examples and feature specification establish these requirements:

- Run as a developer tool on Linux, macOS, and Windows.
- Represent each object as a C struct, with generated methods receiving an explicit `self` pointer.
- Keep initialization separate from allocation so callers can initialize stack storage with `init`, or use a generated heap allocator.
- Generate `deinit` for releasing owned resources and `delete` for deinitializing and freeing heap-allocated objects.
- Support imports, including standard-library imports and imports between `.bam` objects.
- Preserve familiar C declarations and method bodies where possible.

The repository is a prototype with no implementation or established build system. `HexagonFarm.bam` exercises imports, a pointer-to-pointer field, loops, object allocation, ownership, and lifecycle methods. It highlights behaviors the language contract must settle: allocation failure partway through initialization, cleanup of partially initialized objects, and ownership of the pointer array itself.

The input is not ordinary C because imports and object conventions are `bam` syntax. Every approach therefore needs a `bam`-specific front end and explicit semantic rules. A C parser alone cannot decide those rules or guarantee that generated C compiles with a user's chosen compiler.

## Decision

Implement `bam` as a native Rust CLI. Use Tree-sitter's C grammar for C-like declarations and function bodies, with a small `bam` front end for imports and other non-C constructs. Convert accepted input into a project-owned intermediate representation (IR), validate it, then generate deterministic `.h` and `.c` files.

For the first version:

1. Define and document a supported C subset. Reject unsupported and error-recovered syntax with source locations rather than silently guessing.
2. Parse imports and `bam` constructs separately, resolving project imports relative to the importing `.bam` file with normalized paths.
3. Parse fields and method signatures into the IR. Preserve method-body source spans verbatim where no transformation is needed; do not globally replace `self` text.
4. Generate struct declarations, prototypes, imports/includes, and lifecycle/allocation boilerplate from the IR. Preserve explicit `init`/`deinit` method bodies and apply the documented `new`/`delete` conventions.
5. Define supported pointer, array, and function-pointer declarators explicitly. `HexagonFarm.bam` and its generated C both declare the field as `Hexagon **hexagons`.
6. Define failure and ownership semantics for initialization. Generated and user-authored `init`/`deinit` code must have a clear contract for partial initialization, failed allocation, and repeated cleanup.
7. Pin the Tree-sitter runtime and C grammar versions and vendor/use generated parser sources so end users do not need Node.js, the Tree-sitter CLI, or a C compiler just to run `bam`.
8. Build and exercise native release binaries in CI on Linux, macOS, and Windows. Keep generated C portable and validate it with representative C compilers on those platforms.

Keep parsing, validation, IR, and code generation as separate layers. This makes the parser replaceable if the supported language later demands compiler-grade semantic analysis.

## Options considered

### A. Rust CLI, Tree-sitter C grammar, and a `bam` front end — selected

Tree-sitter parses C syntax; the project handles imports, object semantics, validation, and code generation. The generated parser/runtime can be embedded with the CLI.

**Advantages:** Strong syntax coverage and source ranges without requiring users to install a compiler; Rust has supported targets for all three required desktop platforms; clear separation between parser and language-specific rules; useful syntax diagnostics and a migration path to a different parser.

**Costs and risks:** Tree-sitter is syntax-oriented, not a C compiler or type checker. A second front end is still required for `.bam` imports and conventions. Error recovery can produce trees for invalid input, so `bam` must reject error/missing nodes. Complex C declarators and preprocessor behavior need explicit limits. Copying bodies unchanged means Tree-sitter's value is concentrated in declarations and syntax diagnostics.

### B. Rust CLI with a purpose-built restricted parser

Implement a lexer and parser for imports, fields, and method signatures, and retain method bodies as balanced source spans.

**Advantages:** Small runtime and dependency surface; complete control of the accepted dialect and diagnostics; potentially the simplest implementation if `.bam` remains deliberately narrow and bodies are copied unchanged.

**Costs and risks:** Correctly finding top-level declarations requires C-aware tokenization across comments, strings, character literals, escapes, preprocessor lines, and nested delimiters. Declarators are recursive and already nontrivial in `HexagonFarm`. A parser that expands its accepted C subset risks becoming a home-grown C front end. Opaque bodies also defer syntax errors and offer weak source-aware validation. This option is viable if the product clearly promises a small dialect, but its apparent simplicity disappears as syntax support grows.

### C. C++ CLI using Clang LibTooling

Translate or synthesize a C translation unit from `.bam`, then use Clang's AST and diagnostics to analyze declarations and bodies.

**Advantages:** Highest parsing and semantic-analysis capability; compiler-grade handling of C syntax, preprocessing options, types, and source ranges; a strong option if broad compiler compatibility becomes a core requirement.

**Costs and risks:** `.bam` still needs a custom front end and source mapping before Clang can process it. Shipping and building LLVM/Clang components adds significant per-platform complexity and runtime-resource concerns. Clang acceptance does not guarantee output will compile with a different compiler or flags. LibTooling is C++-only and its AST interface can change with LLVM versions. These costs are disproportionate for the current feature set.

## Evaluation and score

Scores use a 1–5 scale (5 is strongest). Weighted criteria reflect this project's needs: cross-platform delivery (25%), fit for current language and transformations (25%), parsing/diagnostic reliability (20%), implementation and release complexity (15%), and ability to evolve (15%).

| Option | Cross-platform 25% | Current fit 25% | Parsing and diagnostics 20% | Complexity 15% | Evolvability 15% | Weighted score |
|---|---:|---:|---:|---:|---:|---:|
| A. Rust + Tree-sitter + `bam` front end | 5 | 4 | 4 | 4 | 4 | **4.25 / 5** |
| B. Rust + restricted custom parser | 5 | 3 | 2 | 4 | 2 | **3.30 / 5** |
| C. C++ + Clang LibTooling | 3 | 5 | 5 | 1 | 4 | **3.75 / 5** |

The scores are comparative judgments for the current repository, not measurements. Option C leads in raw parsing capability; Option A offers the best balance for a portable first implementation without committing to a fragile handwritten C parser or a heavyweight compiler distribution.

## Adversarial review and response

The strongest challenge to Option A is that Tree-sitter may imply broad C compatibility that `bam` cannot actually provide. It does not resolve imported types, expand macros, establish lifecycle safety, or prove generated C is valid under a target compiler. Parsing a method body adds less value if the body is copied unchanged, and imports still require custom logic.

Those objections are valid and shape the decision: Tree-sitter is used as a syntax parser, not as the language definition or compiler. `bam` will explicitly bound supported syntax, reject unsupported/error-recovered constructs, retain method bodies unchanged initially, and validate generated C separately. The IR boundary keeps the parser replaceable. If the intended language is later narrowed to a tiny, stable grammar, Option B should be reconsidered; if arbitrary preprocessing and compiler-grade semantics become required, Option C becomes more compelling.

The adversarial review of Option B found that robust tokenization and C declarators make a custom parser easy to underestimate; keeping it small only works when a narrow dialect is an explicit product decision. The review of Option C found that its AST strengths do not remove the `.bam` translation/source-mapping layer and do not eliminate the distribution burden on each target OS.

## Consequences and follow-up decisions

- `bam` users will need the `bam` executable, but not a C compiler to run the generator. A C compiler is needed separately to build generated output.
- Release engineering must cover native Linux, macOS, and Windows builds. Rust target support includes Windows MSVC and Linux/macOS targets; CI should build and run the CLI on each supported host rather than relying only on cross-compilation.
- The language specification must settle imports, include naming, supported declarators, preprocessor policy, ownership, and partial initialization before those behaviors are treated as stable.
- The current HexagonFarm example needs explicit failure-safe initialization and complete cleanup rules, including freeing the allocated array and handling child allocation failures.
- Generated-code fixtures should be compiled with the platform C compilers in CI, independently of parser tests.

## References

- [Tree-sitter introduction](https://tree-sitter.github.io/tree-sitter/) — concrete syntax trees, error tolerance, and the embedded C11 runtime.
- [Tree-sitter parser generation](https://tree-sitter.github.io/tree-sitter/cli/generate.html) — generated parser output and grammar workflow.
- [Tree-sitter C grammar](https://github.com/tree-sitter/tree-sitter-c) — C grammar source.
- [Rust platform support](https://doc.rust-lang.org/rustc/platform-support.html) — target tiers and platform support.
- [Clang tooling interface comparison](https://clang.llvm.org/docs/Tooling.html) and [LibTooling](https://clang.llvm.org/docs/LibTooling.html) — capabilities and constraints of the Clang alternative.
