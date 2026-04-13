# ARCHITECTURE.md — 3DMMEx

Source port of Microsoft 3D Movie Maker. Version 0.5.0. C++17 codebase with CMake/Ninja build.

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Studio UI Layer                          │
│   (STDIO, Browsers, Easels, Popups, Scrollbars, Scene Sorter)   │
├─────────────────────────────────────────────────────────────────┤
│                     3D Engine Layer                              │
│   (MVIE, SCEN, ACTR, BODY, TMPL, BKGD, TBOX, MSND, TDT/TDF)  │
├─────────────────────────────────────────────────────────────────┤
│                   Kauai Framework Layer                          │
│   (GOB, GOK, APPB, CMH, CRF, CFL, SCPT, SNDM, GNV/GPT)       │
├──────────────────────┬──────────────────────────────────────────┤
│   BRender 3D Engine  │       Platform Abstraction               │
│  (BWLD, BACT, BMDL,  │  (Win32 / SDL2 + miniaudio/FluidSynth/ │
│   BMTL, BPMP, BCAM)  │   GStreamer / Fontconfig / GTK)          │
└──────────────────────┴──────────────────────────────────────────┘
```

---

## Layer 1: Platform Abstraction

Provides cross-platform support via compile-time selection.

### GUI Backends
- **Win32** (`KAUAI_WIN32`): Native Windows API — graphics, input, menus, dialogs, video (VFW)
- **SDL** (`KAUAI_SDL`): SDL2 + SDL2_ttf — graphics, input, menus; miniaudio for sound; FluidSynth for MIDI; GStreamer for video (Linux); NFD for file dialogs

### Platform-Specific Files
Each subsystem has platform variants:
- `*win.cpp` — Windows implementation
- `*sdl.cpp` — SDL implementation
- `*posix.cpp` — POSIX (Linux) implementation
- `*stub.cpp` — Fallback/no-op implementation

### Sound Backends
- **Windows**: AudioMan (original or decompiled) + Windows MIDI stream
- **SDL**: miniaudio (wave) + FluidSynth (MIDI synthesis with SF3 soundfont)

### Video Backends
- **Windows**: Video for Windows (VFW/Vfw32)
- **Linux SDL**: GStreamer pipeline
- **Other**: Stub (no video)

### Font Backends
- **Windows SDL**: System font loading via Win32 API
- **Linux SDL**: Fontconfig-based font discovery
- **Other SDL**: Bundled font fallback

---

## Layer 2: Kauai Framework

A full application framework originally written for 3DMM. Provides everything from memory management to a scripting engine.

### Core Subsystems

| Subsystem | Key Classes | Purpose |
|-----------|-------------|---------|
| **Base** | BASE, ERS, USAC | Run-time type system, error reporting, assertions |
| **Memory** | HQ handles | Handle-based memory with debug tracking |
| **Collections** | GL, AL, GG, AG, GST, AST | Generic lists, groups, string tables |
| **File I/O** | FIL, FNI, BLCK, CFL | File operations, chunky file format |
| **Resources** | CRF, CRM, BACO | Chunky resource files, resource caching |
| **Compression** | CODC, CODM, KCDC | Codec framework (Kauai compression) |
| **Streams** | BSM, BSF | In-memory and file-based byte streams |
| **Scripting** | LEXB, SCCB, SCPT, SCEB | Lexer, compiler, bytecode interpreter |
| **Graphics** | GNV, GPT, NTL, ACR | Graphics environment, ports, palettes |
| **GUI** | GOB, GTE, CMH, CEX | Graphic objects, command dispatch |
| **Application** | APPB | App lifecycle, event loop |
| **Documents** | DOCB, DDG, DMD, UNDB | Document model, display, undo |
| **Kidspace** | GOK, GOKD, WOKS, GKDS | Interactive scene graph with scripting |
| **Rich Text** | TXTB, TXRD, TXPD | Text editing and rich text documents |
| **Controls** | CTL, SCB, WSB | Scrollbars, UI controls |
| **Sound** | SNDM, SNDV, MSTP, MIDS | Sound manager, MIDI, wave playback |
| **Video** | GVID, GVDS | Video playback abstraction |
| **Spell** | SPLC | Spell checker interface |
| **Bitmaps** | MBMP | Masked bitmap format |
| **Vectors** | PIC | Vector graphics |
| **Regions** | REGN, REGSC | Clipping regions |
| **Dialogs** | DLG, DIT | Dialog framework |
| **Cursors** | CURS, CURF | Cursor management |
| **Accelerators** | ATBL | Keyboard shortcuts |
| **Clock** | CLOK | Timer/clock events |

### Kidspace Framework
The most distinctive part of Kauai. Provides an interactive UI system where:
- **GOK** (Kidspace Object) extends GOB with script-driven behavior
- **GOKD** defines object structure (animation states, transitions)
- **WOKS** (World) manages a Kidspace scene with script execution
- **GORP** variants handle visual representation (fill, bitmap, tiled, vector)
- Scripts are compiled to bytecode and run by SCEB/SCEG interpreters

### Chunky File Format
The primary serialization format. A chunky file (CFL) contains:
- Typed chunks identified by CTG (chunk tag) + CNO (chunk number)
- Parent-child relationships between chunks
- Supports embedded resources (bitmaps, sounds, scripts, models)
- Tools: `chomp` (compiler), `ched` (editor), `chmerge` (merger)

---

## Layer 3: 3D Engine

The domain-specific layer for 3D movie creation.

### Core Classes

```
MVIE (Movie — extends DOCB)
 ├── SCEN (Scene) — one per scene in the movie
 │    ├── ACTR[] (Actors) — characters in the scene
 │    │    ├── TMPL (Template/Species) — defines body structure
 │    │    │    ├── ACTN[] (Actions) — animations with cels
 │    │    │    │    └── CEL/CPS — cel frames, body part specs
 │    │    │    └── Costume definitions (CMID per body-part set)
 │    │    ├── BODY (Visual representation)
 │    │    │    ├── BACT[] — BRender actor hierarchy (root + parts)
 │    │    │    ├── MODL[] — 3D models per part
 │    │    │    ├── MTRL[] — Materials per part
 │    │    │    └── CMTL[] — Custom material overrides
 │    │    ├── Route (movement path) — RPT[] points + RTEL location
 │    │    └── Events (AEV[]) — action, costume, sound, rotate, scale, etc.
 │    ├── BKGD (Background)
 │    │    ├── Lights (BACT + BLIT arrays)
 │    │    ├── Cameras (BACT + BCAM)
 │    │    └── Actor placement points
 │    ├── TBOX[] (Text Boxes) — styled text overlays
 │    ├── TDT[] (3D Text) — text rendered as 3D meshes
 │    └── Frame events (sound, pause, camera, transition)
 ├── BWLD (BRender World) — rendering context
 ├── MSQ (Message Sound Queue) — sound playback sync
 ├── TAGM (Tag Manager) — resource resolution
 └── Roll call (actor inventory)
```

### Data Structures

| Struct | Purpose |
|--------|---------|
| XYZ | 3D point (dxr, dyr, dzr) in BRender fixed-point |
| RPT | Route point: XYZ + distance to next |
| RTEL | Route location: node index + offset + frame delta |
| AEV | Actor event: type + frame + route location + variable payload |
| AEVADD | Add-to-stage event (translation + orientation per subroute) |
| AEVACTN | Action change event (action ID + starting cel) |
| AEVCOST | Costume change event (body part set + texture tag) |
| AEVSND | Sound event (loop, volume, cel, motion match) |
| CEL | Animation cel (sound trigger, distance) |
| CPS | Cel part spec (model ID, transform matrix index) |
| XFRM | Current transformation (rotation, scale, path orientation) |
| BDS | Background default sound |
| TAG | Resource tag (file + chunk type + chunk number) |

### Rendering Pipeline

1. **Frame Advance**: `SCEN::FGotoFrm(n)` advances all actors to frame N
2. **Actor Update**: Each `ACTR::FGotoFrame()` processes events, updates route position
3. **Body Position**: `BODY::LocateOrient()` sets BRender actor matrices
4. **Model/Material**: `BODY::SetPartModel()` / `SetPartSetMtrl()` updates visuals
5. **Render**: `BWLD::Render()` calls BRender to render the scene
6. **Callbacks**: `PFNBACTREND` fires per-actor with 2D screen bounds
7. **UI Sync**: Rendered bounds used for hit-testing and selection highlight

### Undo System

Each engine operation has an undo class (all extend UNDB):
- **SUNA** — Actor add/remove
- **SUNC** — Actor costume change
- **SUNK** — Actor action change
- **SUNP** — Path/route change
- **SUNS** — Sound change
- **SUNT** — Text box change

---

## Layer 4: Studio UI

The user-facing interface for creating 3D movies.

### Main Classes

| Class | Purpose |
|-------|---------|
| **APP** (utest.cpp) | Application entry, resolution, splash, global state |
| **STDIO** (studio.cpp) | Central studio UI: tool selection, movie management |
| **SMCC** | Studio-Movie callback bridge |
| **BRWD** (browser.cpp) | Browser base class for asset selection |
| **ESL** (esl.cpp) | Easel base for editing sub-UIs |
| **SSCB** | Scene/frame timeline scrollbars |
| **SCRT** (scnsort.cpp) | Scene sorter for reordering |
| **APE** (ape.cpp) | Actor preview entity |
| **MP/MPFNT** (popup.cpp) | Popup menus for fonts/colors |
| **TGOB** (tgob.cpp) | Text display objects |
| **SPLOT** (splot.cpp) | Splash screen transitions |
| **TATR** (tatr.cpp) | Theater (playback-only mode) |

### Browser Hierarchy

```
BRWD (base)
├── BRWL (list-based, chunky file)
│   ├── BRWP (Props/Actors)
│   ├── BRWB (Backgrounds)
│   ├── BRWC (Cameras)
│   └── BRWN (Named list)
│       ├── BRWM (Music)
│       └── BRWI (Import Sounds)
├── BRWT (text-based)
│   └── BRWA (Actions)
└── BRWR (Roll Call)
```

### Easel Hierarchy

```
ESL (base)
├── ESLA (Actor editing)
├── ESLT (Text editing)
├── SNE  (Sound editing)
├── ESLL (Listener properties)
├── LSND (Listener sound)
└── ESLR (Recording)
```

### Building Mode

Separate UI for front-end navigation (not the 3D editor):
- **Lobby** — Main hub
- **Backstage** — Character/costume selection
- **Projects** — Project listing
- **Theater** — Movie preview
- **Inspiration** — Help/ideas
- **Ticket/Loader** — Loading screens
- **Snackbar** — Navigation/help

---

## BRender Integration

BRender is the 3D rendering backend. Two modes:
- **Original**: Precompiled static libraries (x86 MSVC only)
- **Source**: Built from https://github.com/benstone/3DMM-BRender (branch: blazin)

### Type Mappings

| 3DMMEx | BRender | Purpose |
|--------|---------|---------|
| BACT | br_actor | Scene graph node |
| BMDL | br_model | 3D mesh (vertices + faces) |
| BMTL | br_material | Surface properties |
| BLIT | br_light | Light source |
| BCAM | br_camera | Camera/view |
| BPMP | br_pixelmap | Texture/bitmap |
| BMAT34 | br_matrix34 | 4x3 transformation matrix |
| BRS | br_scalar | Fixed-point scalar |
| BRA | br_angle | Fixed-point angle |
| BVEC3 | br_vector3 | 3D vector |

### Key Rendering Classes

| Class | File | Purpose |
|-------|------|---------|
| BWLD | bren/inc/bwld.h | World: scene root, camera, render buffers, callbacks |
| TMAP | bren/inc/tmap.h | Texture map management |
| ZBMP | bren/inc/zbmp.h | Z-buffer bitmap (16-bit depth) |
| bren wrapper | bren/inc/bren.h | Type aliases + on-disk format definitions |

---

## Execution Model

- **Single-threaded**, event-driven architecture
- No background rendering threads
- Frame advancement is synchronous: `FGotoFrame()` updates all state atomically
- Sound playback is asynchronous via platform audio APIs (AudioMan/miniaudio)
- MIDI synthesis runs in platform audio thread (Windows MIDI stream / FluidSynth)
- UI updates via Kauai command dispatch (CMH/CEX)
- Script execution is synchronous within frame processing

---

## Serialization

All project data uses the **Chunky file format**:
- `.3mm` — Movie project files
- `.3cn` — Chunky content (backgrounds, materials, sounds, templates)
- `.3th` — Chunky resource headers (actor, background, material, prop, sound definitions)
- `.cht` — Chunky text (script/resource definitions for UI, help)
- `.chh` — Chunky header includes

Resources are identified by **TAG** (file reference + CTG + CNO) and resolved at runtime by **TAGM** (Tag Manager).

---

## Content Pipeline

```
Source assets (.hrc, .bmp, .wav, .mid)
        │
        ▼
Conversion tools (sitobren, tdfmake, pbmtobmp, mktmap)
        │
        ▼
Chunky text files (.cht)
        │
        ▼
Chomp compiler → Chunky binary (.3cn, .3th, .chk)
        │
        ▼
Runtime resource loading via CRF/TAGM
```

---

## Test Infrastructure

- **Framework**: Google Test v1.15.0
- **KauaiTest**: Core tests (compression, compiler, strings, math)
- **KauaiGuiTest**: GUI component tests
- **khello**: Minimal GUI test application
- **CI**: GitHub Actions with multi-platform matrix (MSVC x86/x64, Clang, GCC Linux)
