# PROJECT_RULES.md — 3DMMEx Development Rules

Build conventions, coding patterns, platform rules, and audit notes.

---

## Build System

- **CMake 3.22+** with Ninja generator
- **C++17** standard (`CMAKE_CXX_STANDARD 17`)
- MSVC runtime: `MultiThreaded` (Release) / `MultiThreadedDebugDLL` (Debug)
- Interprocedural optimization enabled for RelWithDebInfo
- Code style: Microsoft (`.clang-format`), **include order is never sorted** (critical)

---

## Platform Selection

The project uses compile-time platform selection via CMake options:

| Decision | Option | Values |
|----------|--------|--------|
| GUI backend | `3DMM_GUI` | Win32, SDL |
| 3D renderer | `3DMM_BRENDER_LIBRARY` | Original (precompiled), Source (built) |
| Audio mixer | `3DMM_AUDIOMAN_LIBRARY` | Original, Decompilation, None |
| Unicode | `3DMM_UNICODE` | ON/OFF |
| x86 ASM | `ENABLE_ASM_X86` | ON/OFF |

### Platform File Patterns

Each subsystem uses platform-specific source files:
```
feature.cpp        # Shared/interface code
featurewin.cpp     # Windows (KAUAI_WIN32)
featuresdl.cpp     # SDL (KAUAI_SDL)
featureposix.cpp   # POSIX/Linux
featurestub.cpp    # No-op fallback
featuremac.cpp     # Legacy Mac (unused)
```

CMakeLists.txt selects files at configure time based on `3DMM_GUI` and platform.

---

## Naming Conventions

### Types
- **ALL CAPS**: Class names (ACTR, SCEN, MVIE, BODY, GOB, CMH)
- **P prefix**: Pointer typedef (PACTR = ACTR*, PMVIE = MVIE*)
- **PGL/PGG/PGST**: Pointer to GL/GG/GST collection
- **F prefix**: Boolean-returning methods (FGotoFrame, FAddActr, FSetAction)
- **P prefix on factory methods**: Returns pointer (PactrNew, PscenNew, PmvieNew)
- **CTG**: Chunk tag (4-char type identifier)
- **CNO**: Chunk number
- **TAG**: Resource reference (file + CTG + CNO)

### Variables
- **_ prefix**: Member variables (_pmvie, _pbody, _nfrmCur)
- **p prefix**: Pointer (pactr, pscen, pfni)
- **c prefix**: Count (cactn, ccam, celn)
- **n prefix**: Number/index (nfrm, nfrm)
- **f prefix**: Boolean flag (fLoop, fPlaying, fFreeze)
- **gr prefix**: Bit flags (grfactn, grfmaf)
- **dxr/dyr/dzr**: BRender coordinate deltas
- **cb prefix**: Byte count
- **ib prefix**: Index into array

### Files
- `.cpp/.h` — C++ source/header
- `.cht` — Chunky text (script resource definition)
- `.chh` — Chunky header (include for .cht files)
- `.3mm` — Movie project file
- `.3cn` — Chunky content container
- `.3th` — Chunky resource header
- `.rc` — Windows resource script

---

## Runtime Type System

All classes derive from BASE which provides:
- **RTCLASS** macro for runtime type checking
- **AssertValid()** for debug assertions
- **MarkMem()** for memory tracking
- Reference counting via **AddRef()** / **Release()**

---

## Chunky File Format Rules

- Chunks are identified by CTG (4-char tag) + CNO (number)
- Chunks can have parent-child relationships
- Resources are loaded lazily via CRF (Chunky Resource File)
- Resource tags (TAG) are resolved at runtime by TAGM
- The `chomp` tool compiles `.cht` text files to binary chunky format
- `.cht` files use a Kauai-specific scripting syntax

---

## Undo System

Every user-visible operation must create an undo record:
- Undo classes extend UNDB (undo base from Kauai)
- Engine undo classes: SUNA (actor), SUNC (costume), SUNK (action), SUNP (path), SUNS (sound), SUNT (text)
- Undo records store before/after state for rollback
- The undo stack is linear (no branching)

---

## Event-Driven Architecture

- Single-threaded execution model
- Commands dispatched via CMH (Command Handler) → CEX (Command Executor)
- Script bytecode runs synchronously within frame processing
- Sound playback is the only asynchronous operation (via platform audio APIs)
- Frame advancement is atomic: `FGotoFrame()` processes all events sequentially

---

## BRender Integration Rules

- BRender types are aliased (BACT = br_actor, BMDL = br_model, etc.)
- Fixed-point math: BRS (br_scalar), BRA (br_angle)
- Scene graph: BACT tree managed by BODY class
- Models and materials managed separately from scene graph
- Two build modes: Original (precompiled x86 libs) and Source (cross-platform)
- BWLD (world) owns the render context, camera, and buffers

---

## Memory Management

- Handle-based allocation (HQ handles) via Kauai
- Debug builds track allocations and report leaks
- Collection classes (GL, GG, GST) manage dynamic arrays
- Reference counting for resource objects (CRF-loaded)
- No smart pointers (C++17 but legacy codebase)

---

## Testing

- Google Test v1.15.0 framework
- **KauaiTest**: Core functionality (math, compression, compiler, strings)
- **KauaiGuiTest**: GUI components
- Run via: `ctest --test-dir build/<preset>`
- CI runs tests with 60-second timeout per test
- Test resources in `kauai/test/res/`

---

## CI/CD Pipeline

### build.yml Matrix
| Configuration | Preset | Targets |
|---------------|--------|---------|
| MSVC x86 Release | x86-msvc-relwithdebinfo | studio, tools, tests |
| Clang x86 Debug | x86-clangcl-debug | studio, tests |
| MSVC x64 Debug | x64-msvc-debug | studio, tests |
| SDL MSVC x64 Debug | sdl-x64-msvc-debug | studio, tests |
| Clang x64 Release | x64-clangcl-relwithdebinfo | studio, tests |
| MSVC ARM64 Release | arm64-msvc-relwithdebinfo | studio (public repo only) |
| GCC Linux SDL Debug | sdl-x86_64-gcc-linux-debug | studio, tests |

### clang-format-check.yml
- Enforces code style on all `.cpp`, `.c`, `.h` files
- Fails PR if formatting changes needed

---

## Docker Build

- Base: Windows Server Core LTSC 2019
- Visual Studio 2022 Build Tools + C++ workload
- CMake + Ninja via Chocolatey
- Builds MSVC x86 Debug preset by default

---

## Key Patterns for Audit

### Large Files (potential refactoring targets)
| File | Size | Class | Lines |
|------|------|-------|-------|
| src/engine/movie.cpp | 207KB | MVIE | ~5000+ |
| src/engine/actor.cpp | 167KB | ACTR | ~4000+ |
| src/engine/scene.cpp | 166KB | SCEN | ~4000+ |
| src/engine/tbox.cpp | 79KB | TBOX | ~2000+ |
| src/studio/utest.cpp | — | APP | 4,762 |
| src/studio/browser.cpp | — | BRWD+ | 3,668 |
| src/studio/studio.cpp | — | STDIO | 2,859 |

### Legacy Code Patterns
- No smart pointers (raw `new`/`delete` and handle-based allocation)
- Macro-heavy (RTCLASS, AssertValid, etc.)
- 4-character type tags (CTG) as identifiers
- Fixed-point math (BRender BRS/BRA) instead of floating-point
- No namespaces (all global scope)
- Abbreviation-heavy naming (ACTR, SCEN, MVIE, BKGD, TMPL)
- Mac-era code patterns (handle-based memory, resource forks → chunky files)

### Cross-Platform Status
- **Complete**: Win32 GUI, SDL GUI (graphics, input, menus)
- **Complete**: Sound (Win32 AudioMan/MIDI, SDL miniaudio/FluidSynth)
- **Complete**: Video (Win32 VFW, Linux GStreamer)
- **In Progress**: Linux platform support
- **Not Started**: macOS (legacy Mac code exists but not functional)

### Potential Security Concerns
- File parsing (chunky format, AVI, BRender models) — potential buffer overflows
- No input sanitization on file paths in FNI
- Legacy codec decompression — potential memory safety issues
- No ASLR/DEP considerations in original code
- Fixed-size buffers in string operations (STN class)
