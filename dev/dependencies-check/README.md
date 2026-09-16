# dependencies-check

Internal development tool for detecting **circular dependencies** within the
Rustberg workspace. It leverages Cargo's own resolver to build a dependency
graph and runs a DFS-based cycle detector — catching a class of build issues
that `cargo check` alone cannot surface.

---

## Table of Contents

- [Why This Exists](#why-this-exists)
- [How It Works](#how-it-works)
- [Architecture Overview](#architecture-overview)
- [Detailed Flow](#detailed-flow)
  - [1. Workspace Resolution](#1-workspace-resolution)
  - [2. Dependency Graph Construction](#2-dependency-graph-construction)
  - [3. Cycle Detection (DFS)](#3-cycle-detection-dfs)
- [ASCII Walkthrough](#ascii-walkthrough)
- [Usage](#usage)
- [Example Output](#example-output)
- [Limitations](#limitation)

---

## Why This Exists

In a large workspace with many interdependent crates, it is easy to
accidentally introduce a cycle:

```
    ┌──────────────┐
    │  crate-a     │──────► crate-b
    └──────────────┘              │
           ▲                      │
           │          ┌───────────┘
           ▼          ▼
    ┌──────────────┐  ┌──────────────┐
    │  crate-c     │◄─│  crate-d     │
    └──────────────┘  └──────────────┘
         ... cycle?
```

Cargo's own resolver will usually catch hard cycles at build time, but **soft
cycles** (e.g. via feature-gated optional deps, or proc-macro expansion order)
can manifest as mysterious build failures. This tool proactively detects the
hard cycles **before** you even run `cargo build`.

---

## How It Works

The tool uses three phases:

```
    ┌─────────────────────────────────────────────────────────────────┐
    │                      PHASE 1: RESOLVE                          │
    │  cargo::ops::resolve_ws() → full dependency tree               │
    └───────────────────────────────┬─────────────────────────────────┘
                                    │
                                    ▼
    ┌─────────────────────────────────────────────────────────────────┐
    │                    PHASE 2: FILTER + MAP                       │
    │  Extract packages matching the target prefix                   │
    │  Build HashMap<Package, Vec<Dependency>>                       │
    └───────────────────────────────┬─────────────────────────────────┘
                                    │
                                    ▼
    ┌─────────────────────────────────────────────────────────────────┐
    │                   PHASE 3: DFS CYCLE CHECK                     │
    │  For each package, run recursive DFS with a "seen" set         │
    │  If we revisit the root → PANIC with cycle details             │
    └─────────────────────────────────────────────────────────────────┘
```

---

## Architecture Overview

```
    ┌─────────────────────────────────────────────────────────────┐
    │                   dev/dependencies-check                    │
    │                        (this crate)                         │
    │                                                             │
    │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
    │  │   main.rs     │  │               │  │               │   │
    │  │               │  │  (single      │  │               │   │
    │  │  - resolve    │──│   binary,     │  │               │   │
    │  │  - filter     │  │   no lib)     │  │               │   │
    │  │  - detect     │  │               │  │               │   │
    │  └───────┬───────┘  └───────────────┘  └───────────────┘   │
    │          │                                                  │
    │          │ uses                                             │
    │          ▼                                                  │
    │  ┌───────────────────┐                                      │
    │  │  cargo crate      │                                      │
    │  │  (0.99.0)         │                                      │
    │  │                   │                                      │
    │  │  - GlobalContext   │                                      │
    │  │  - Workspace       │                                      │
    │  │  - resolve_ws()   │                                      │
    │  │  - DependencyGraph│                                      │
    │  └───────────────────┘                                      │
    │          │                                                  │
    └──────────┼──────────────────────────────────────────────────┘
               │
               │ reads
               ▼
    ┌───────────────────────────┐
    │  ../../Cargo.toml         │
    │  (workspace root)         │
    │                           │
    │  members = [              │
    │    "rustberg-common",     │
    │    "rustberg-core",       │
    │    "rustberg-optimizer",  │
    │    "rustberg-proto",      │
    │    ...                    │
    │  ]                        │
    │  exclude = [              │
    │    "dev/dependencies-check"│ ◄── self-excluded
    │  ]                        │
    └───────────────────────────┘
```

---

## Detailed Flow

### 1. Workspace Resolution

```rust
let gctx = GlobalContext::default()?;
let workspace = cargo::core::Workspace::new(&root_cargo_toml, &gctx)?;
let (_, resolve) = cargo::ops::resolve_ws(&workspace, false)?;
```

The tool locates the workspace root by walking up from `CARGO_MANIFEST_DIR`:

```
    CARGO_MANIFEST_DIR
    = .../rustberg/dev/dependencies-check
                    │
                    ├── .parent() → dev/
                    │                  │
                    │                  └── .parent() → rustberg/
                    │                                    │
                    │                                    └── Cargo.toml
                    │                                       (workspace root)
                    ▼
              ┌─────────────────────────┐
              │  Workspace::new()       │
              │                         │
              │  Parses Cargo.toml,     │
              │  discovers all member   │
              │  crates, resolves full  │
              │  dependency graph       │
              └─────────────────────────┘
```

`resolve_ws()` returns a `Resolve` struct — a **complete snapshot** of the
dependency graph including all transitive dependencies, feature flags, and
platform-specific selections.

### 2. Dependency Graph Construction

```rust
for package_id in resolve.iter()
    .filter(|id| id.name().starts_with("target-prefix"))
{
    let deps: Vec<String> = resolve
        .deps(package_id)
        .filter(|(package_id, _)| package_id.name().starts_with("target-prefix"))
        .map(|(package_id, _)| package_id.name().to_string())
        .collect();
    package_deps.insert(package_id.name().to_string(), deps);
}
```

This builds an **adjacency list** representation of the graph, filtered to
only crates matching the configured prefix:

```
    Full resolve graph (simplified):
    ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
    │ rustberg-core│────►│ target-pkg-a │────►│ target-pkg-b │
    └──────────────┘     └──────┬───────┘     └──────┬───────┘
                                │                     │
                                │                     │
                         ┌──────▼───────┐      ┌─────▼────────┐
                         │ target-pkg-c │      │ target-pkg-d │
                         └──────┬───────┘      └──────────────┘
                                │
                                │

    Filtered to target prefix only:

    ┌─────────────────────────────────────────────────────┐
    │  HashMap<String, Vec<String>>                       │
    │                                                     │
    │  "target-pkg-a" ──► ["target-pkg-b",               │
    │                       "target-pkg-c"]               │
    │                                                     │
    │  "target-pkg-b" ──► ["target-pkg-d"]               │
    │                                                     │
    │  "target-pkg-c" ──► ["target-pkg-a"]               │
    │                                                     │
    │  "target-pkg-d" ──► []                              │
    │  ...                                                │
    └─────────────────────────────────────────────────────┘
```

### 3. Cycle Detection (DFS)

```rust
for (root_package, deps) in &package_deps {
    let mut seen = HashSet::new();
    for dep in deps {
        check_circular_deps(root_package, dep, &package_deps, &mut seen);
    }
}
```

The algorithm is a classic **recursive DFS** with path tracking:

```
    check_circular_deps(root, current, graph, seen):

    ┌──────────────────────────────────────────────┐
    │  IF current == root:                         │
    │     ◄── CYCLE FOUND! Panic with path.        │
    │                                               │
    │  IF current ∈ seen:                           │
    │     ◄── Already visited from this root.       │
    │         Skip (avoid infinite recursion).      │
    │                                               │
    │  seen.insert(current)                         │
    │                                               │
    │  FOR EACH dep ∈ graph[current]:               │
    │     check_circular_deps(root, dep, seen)      │
    └──────────────────────────────────────────────┘
```

**Walkthrough with example:**

```
    Starting: root = "target-pkg-a"
    Checking dep "target-pkg-c"...

    DFS Stack Visualization:
    ┌───────────────────────────────────────────────────────┐
    │                                                       │
    │  Stack frame 1:  root="target-pkg-a"                 │
    │    current="target-pkg-c"                            │
    │    seen = {}                                          │
    │         │                                             │
    │         ▼                                             │
    │  Stack frame 2:  root="target-pkg-a"                 │
    │    current="target-pkg-a"  ◄── MATCH! ROOT!          │
    │    seen = {"target-pkg-c"}                           │
    │                                                       │
    │  PANIC: "circular dependency detected from            │
    │          target-pkg-a to self via one of              │
    │          ["target-pkg-c"]"                            │
    │                                                       │
    └───────────────────────────────────────────────────────┘
```

---

## ASCII Walkthrough

Full example — what happens when you run the tool:

```
    $ cargo run -p dependencies-check

    ┌─────────────────────────────────────────────────────────┐
    │                                                         │
    │  Step 1: Resolve workspace                              │
    │  ─────────────────────────                              │
    │  Reading: .../rustberg/Cargo.toml                       │
    │  Found 12 workspace members                             │
    │  Resolving 847 total packages...                        │
    │                                                         │
    │          ┌─────────────────────────┐                    │
    │          │      Cargo.lock          │                    │
    │          │  ┌───────────────────┐  │                    │
    │          │  │ target-pkg-a     │──┼──► target-pkg-b    │
    │          │  │                   │──┼──► target-pkg-c    │
    │          │  │                   │──┼──► ...             │
    │          │  └───────────────────┘  │                    │
    │          └─────────────────────────┘                    │
    │                                                         │
    │  Step 2: Filter crates                                  │
    │  ──────────────────────                                 │
    │  Found 32 crates matching target prefix                 │
    │                                                         │
    │  Step 3: Check for cycles                               │
    │  ─────────────────────────                              │
    │  Checking target-pkg-a         ... ✓                    │
    │  Checking target-pkg-b         ... ✓                    │
    │  Checking target-pkg-c         ... ✓                    │
    │  Checking target-pkg-d         ... ✓                    │
    │  ...                                                    │
    │  Checking target-pkg-z         ... ✓                    │
    │                                                         │
    │  No circular dependencies found                         │
    │                                                         │
    └─────────────────────────────────────────────────────────┘
```

---

## Usage

This crate is **excluded** from the workspace (`exclude = ["dev/dependencies-check"]`)
so it is not built by `cargo build` at the project root. Run it directly:

```bash
# From project root
cd dev/dependencies-check
cargo run

# Or with release optimizations
cargo run --release

# Or build the binary first
cargo build --release
./target/release/dependencies-check
```

**Output:**

```
Checking for circular dependencies in /path/to/rustberg/Cargo.toml
No circular dependencies found
```

If a cycle is found, the process **panics** with the cycle path:

```
thread 'main' panicked at 'circular dependency detected from
target-pkg-a to self via one of ["target-pkg-c"]'
```

---

## Example Output

**Success case:**

```
$ cd dev/dependencies-check && cargo run
   Compiling dependencies-check v0.1.0 (.../dev/dependencies-check)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.34s
     Running `target/debug/dependencies-check`
Checking for circular dependencies in .../rustberg/Cargo.toml
No circular dependencies found
```

**Cycle detected (hypothetical):**

```
$ cd dev/dependencies-check && cargo run
     Running `target/debug/dependencies-check`
Checking for circular dependencies in .../rustberg/Cargo.toml
thread 'main' panicked at 'circular dependency detected from
target-pkg-a to self via one of ["target-pkg-c",
"target-pkg-d"]', src/main.rs:63:9
```

---

## Limitations

| Aspect | Detail |
|--------|--------|
| **Scope** | Currently filters crates by a hardcoded prefix. Can be modified in `src/main.rs` to target different crate groups. |
| **Cycle type** | Detects **hard** dependency cycles only (direct crate → crate edges). Does not detect feature-flag-mediated or build-script-mediated cycles. |
| **Exit code** | Panics on cycle (exit code 101). No machine-readable output (no JSON/CI integration). |
| **Performance** | O(V × (V + E)) worst case due to per-root DFS. Fine for ~30 crates; not designed for larger graphs. |
| **Workspace root** | Relies on `CARGO_MANIFEST_DIR` env var to find workspace root. Will fail if run from an unexpected location. |
