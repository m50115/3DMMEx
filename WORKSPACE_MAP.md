# WORKSPACE_MAP.md — 3DMMEx Directory Guide

Quick reference for navigating the codebase.

---

## Root Structure

```
3DMMEx/
├── CMakeLists.txt          # Root build file (project config, options, dependencies)
├── CMakePresets.json        # Build presets (MSVC/Clang/GCC, x86/x64/ARM64, Debug/Release)
├── Dockerfile               # Windows Server Core container build
├── docker-compose.yaml      # Docker build service
├── .clang-format            # Code style (Microsoft base, no include sorting)
├── .github/workflows/       # CI/CD
│   ├── build.yml            # Multi-platform build + test matrix
│   └── clang-format-check.yml  # Style enforcement
│
├── bren/                    # BRender 3D engine integration
├── cmake/                   # CMake modules (Find*, Fetch*, generators)
├── content-files/           # Runtime content (AVI, chunky containers, resource headers)
├── doc/                     # Historical documentation from 1995
├── elib/                    # External precompiled libraries (BRender)
├── img/                     # Project images (screenshots, etc.)
├── inc/                     # Application-level headers
├── kauai/                   # Kauai framework (core engine)
├── projects/                # Sample 3DMM project files (6 .3mm)
├── samples/                 # Example movie files (16 .3mm)
├── src/                     # Application source code
└── tools/                   # Content pipeline tools
```

---

## /bren/ — BRender Integration

```
bren/
├── inc/
│   ├── bren.h       # Type aliases (BACT, BMDL, BMTL, etc.), on-disk formats
│   ├── bwld.h       # BWLD class: 3D world, camera, render buffers, callbacks
│   ├── tmap.h       # TMAP: texture map management
│   ├── zbmp.h       # ZBMP: 16-bit Z-buffer bitmap
│   └── fwpextra.h   # (empty)
└── CMakeLists.txt   # Builds 'bren' library → links KauaiBase + BRender::Libraries
```

---

## /cmake/ — Build Modules

| Module | Purpose |
|--------|---------|
| ExtractVersion.cmake | Parse version from tool output |
| FetchFluidSynth.cmake | FluidSynth 2.5.2 + MuseScore soundfont (Windows) |
| FetchGoogleTest.cmake | Google Test v1.15.0 |
| FetchIniParser.cmake | INI config parser (SDL GUI) |
| FetchMiniaudio.cmake | miniaudio 0.11.23 (SDL audio) |
| FetchNativeFileDialogExtended.cmake | Cross-platform file dialogs (SDL) |
| FetchSDL2.cmake | SDL2 2.32.6 + SDL2_ttf 2.24.0 (Windows) |
| FindAudioMan.cmake | Locate precompiled AudioMan |
| FindBRender.cmake | Locate precompiled BRender (x86 MSVC) |
| FindCCache.cmake | ccache compiler cache |
| FindClangTidy.cmake | clang-tidy static analysis |
| FindFluidSynth.cmake | System FluidSynth |
| GenerateChunkTags.cmake | Generate chunk tag macros from text files |
| TargetChompSources.cmake | Chomp preprocessor for .cht files |

---

## /content-files/ — Runtime Assets (37 files)

| Category | Files | Format |
|----------|-------|--------|
| Videos | 24 files (logn*.avi, proj*.avi, idea*.avi, etc.) | AVI |
| Chunky containers | bkgds.3cn, mtrls.3cn, snds.3cn, tdfs.3cn, tmpls.3cn | 3CN |
| Resource headers | actor.3th, actresl.3th, bkgds.3th, mtrl.3th, prop.3th, sound.3th, tbox.3th | 3TH |
| Metadata | 3dmovie.ms | MS |

Installed to: `Microsoft Kids/3D Movie Maker/`

---

## /elib/ — External Precompiled Libraries

```
elib/
├── brender/
│   ├── inc/         # 44 BRender header files (actors, models, materials, etc.)
│   ├── wind/        # Windows Debug BRender libraries
│   └── wins/        # Windows Release BRender libraries
```

---

## /inc/ — Application Headers

All headers for the engine and studio layers. Key files:

| Header | Defines |
|--------|---------|
| actor.h | ACTR, routes, events, AEV types |
| scene.h | SCEN, frame events, undo classes |
| movie.h | MVIE, tools enum, MCC callback interface |
| body.h | BODY, body parts, hit testing |
| tmpl.h | TMPL, ACTN, costumes |
| bkgd.h | BKGD, lights, cameras, placement |
| mtrl.h | MTRL, CMTL materials |
| modl.h | MODL model wrapper |
| msnd.h | MSND movie sounds |
| tbox.h | TBOX text boxes |
| tdt.h / tdf.h | TDT 3D text / TDF 3D fonts |
| studio.h | STDIO studio class |
| browser.h | BRWD browser hierarchy |
| esl.h | ESL easel hierarchy |
| soc.h | APP main application |
| tagman.h / tagl.h | TAG resource management |
| portf.h | File dialogs |
| version.h.in | Version template (generated) |
| sitobren.h | SoftImage-to-BRender conversion |
| helpbook.h / helpres.h | Help system |
| kidgs*.h / kidsanim.h | Kidspace definitions |

---

## /kauai/ — Kauai Framework

The core framework. ~185 source files across:

```
kauai/
├── CMakeLists.txt    # Builds 26+ library targets
├── src/              # Framework source (~80 .cpp, ~70 .h)
│   ├── base.cpp/h         # BASE class, runtime type
│   ├── util*.cpp/h        # Utilities (memory, strings, integers, hex, random)
│   ├── groups*.cpp/h      # Collections (GL, GG, GST)
│   ├── file*.cpp/h        # File I/O + platform variants
│   ├── fni*.cpp/h         # File name interface + platform variants
│   ├── chunk.cpp/h        # Chunky file format (CFL)
│   ├── crf.cpp/h          # Resource files (CRF, CRM)
│   ├── codec*.cpp/h       # Compression
│   ├── stream.cpp/h       # Byte streams
│   ├── lex.cpp/h          # Script lexer
│   ├── scrcom*.cpp/h      # Script compiler
│   ├── screxe*.cpp/h      # Script interpreter
│   ├── gfx*.cpp/h         # Graphics (+ platform variants)
│   ├── gob*.cpp/h         # Graphic objects (+ platform variants)
│   ├── appb*.cpp/h        # Application (+ platform variants)
│   ├── cmd.cpp/h          # Command system
│   ├── docb.cpp/h         # Document framework
│   ├── text*.cpp/h        # Text editing
│   ├── rtxt*.cpp/h        # Rich text
│   ├── kidspace.cpp/h     # Kidspace scene graph
│   ├── kidworld.cpp/h     # Kidspace world
│   ├── kidhelp.cpp/h      # Help balloons
│   ├── sndm.cpp/h         # Sound manager
│   ├── midi*.cpp/h        # MIDI (+ platform variants)
│   ├── sndma*.cpp/h       # miniaudio backend
│   ├── sndam.cpp/h        # AudioMan backend
│   ├── video*.cpp/h       # Video (+ platform variants)
│   ├── font*.cpp/h        # Font loading (SDL variants)
│   ├── mbmp*.cpp/h        # Masked bitmaps
│   ├── pic*.cpp/h         # Vector graphics
│   ├── region.cpp/h       # Clipping regions
│   ├── dlg*.cpp/h         # Dialogs
│   ├── ctl.cpp/h          # Controls
│   ├── cursor*.cpp/h      # Cursors (+ platform variants)
│   ├── menu*.cpp/h        # Menus (+ platform variants)
│   ├── clok.cpp/h         # Timer/clock
│   ├── clip.cpp/h         # Clipboard
│   ├── spell.cpp/h        # Spell checker
│   ├── chse.cpp/h         # Chunky source emitter
│   ├── chcm.cpp/h         # Chunky compiler
│   ├── accelerator.cpp/h  # Keyboard shortcuts
│   └── platform.h + plat*.cpp  # Platform abstraction
│
├── doc/              # Kauai documentation
├── elib/             # Kauai-specific external libs
│   ├── wind/         # Windows Debug libs
│   ├── wins/         # Windows Release libs
│   ├── winud/        # Windows Unicode Debug
│   └── winus/        # Windows Unicode Release
├── test/             # Test suite
│   ├── kauai_test.cpp       # Math/utility tests
│   ├── compression_test.cpp # Codec tests
│   ├── compiler_test.cpp    # Chunky compiler tests
│   ├── string_test.cpp      # String tests
│   ├── gui_test.cpp         # GUI component tests
│   ├── khello/              # Hello World test app
│   └── res/                 # Test resources
└── tools/            # Kauai development tools
    ├── chomp.cpp     # Chunky file compiler (cross-platform)
    ├── ched.cpp      # Chunky editor (Win32 GUI)
    ├── chelp.cpp     # Help editor (Win32 GUI)
    ├── chmerge.cpp   # Chunky file merger
    ├── chelpdmp.cpp  # Help content dumper
    ├── mkmbmp.cpp    # Masked bitmap maker
    ├── kpack.cpp     # Resource packer
    ├── ft.cpp        # Frame tester (Win32)
    └── ut.cpp        # Utility tester
```

---

## /src/ — Application Source

```
src/
├── CMakeLists.txt         # Builds 'engine' library + 'studio' executable
├── engine/                # 3D engine implementation
│   ├── actor.cpp (167KB)  # ACTR: animation, routes, events, costumes
│   ├── actredit.cpp       # ACTR editing: path recording, route manipulation
│   ├── actrsave.cpp       # ACTR file I/O
│   ├── actrsnd.cpp        # ACTR sound: motion-match, volume
│   ├── scene.cpp (166KB)  # SCEN: frame control, actors, text, sound events
│   ├── movie.cpp (207KB)  # MVIE: document, playback, roll call, undo
│   ├── body.cpp           # BODY: BRender actor tree, materials, hit detection
│   ├── tmpl.cpp           # TMPL: actor templates, actions, costumes
│   ├── bkgd.cpp           # BKGD: backgrounds, lights, cameras
│   ├── modl.cpp           # MODL: BRender model wrapper
│   ├── mtrl.cpp           # MTRL/CMTL: materials
│   ├── tbox.cpp           # TBOX: text boxes
│   ├── tdt.cpp            # TDT: 3D text objects
│   ├── tdf.cpp            # TDF: 3D font definitions
│   ├── msnd.cpp           # MSND: movie sound objects
│   ├── tagman.cpp         # TAGM: resource tag management
│   ├── tagl.cpp           # TAGL: tag list container
│   ├── srec*.cpp          # Scene recording (platform-specific)
│   └── srecma.cpp         # miniaudio scene recording
│
├── studio/                # Studio UI
│   ├── studio.cpp (2.9KB) # STDIO: central UI coordinator
│   ├── utest.cpp (4.8KB)  # APP: initialization, resolution, splash
│   ├── browser.cpp (3.7KB)# BRWD: all browser implementations
│   ├── esl.cpp (2KB)      # ESL: easel framework
│   ├── portfwin.cpp       # Windows file dialogs
│   ├── portfnative.cpp    # NFD file dialogs (SDL)
│   ├── scnsort.cpp        # SCRT: scene sorter
│   ├── ape.cpp            # APE: actor preview
│   ├── popup.cpp          # MP/MPFNT: popup menus
│   ├── splot.cpp          # SPLOT: splash transitions
│   ├── tatr.cpp           # TATR: theater (playback)
│   ├── configini.cpp      # INI config persistence
│   ├── mminstal.cpp       # Codec/driver detection
│   ├── tgob.cpp           # TGOB: text objects
│   ├── dlgsdl.cpp         # SDL dialog support
│   ├── stdiobrw.cpp       # Studio browser integration
│   ├── stdioscb.cpp       # Studio scrollbar integration
│   ├── utestscb.cpp       # SSCB: scrollbar class
│   ├── appicon.cpp        # Embedded app icon
│   ├── *.cht              # Chunky script/resource files
│   ├── bmp/               # Studio bitmaps
│   ├── cur/               # 60 cursor files
│   └── sound/             # Studio sounds
│
├── building/              # Building mode (front-end navigation)
│   ├── *.cht              # Chunky resources per area
│   ├── *.seq              # Animated transitions
│   ├── bitmaps/           # Area-specific bitmaps
│   ├── pbm/               # Platform bitmap data
│   └── sound/             # Area-specific sounds
│
├── shared/                # Shared between Studio and Building
│   ├── *.cht              # Shared chunky resources
│   ├── bmp/               # 38 shared bitmaps
│   ├── pbm/               # 52 platform bitmaps
│   ├── cursors/           # 15 shared cursors
│   └── sound/             # 12 shared sounds + subdirs (bio, map, util)
│
├── help/                  # Help content
│   ├── app.cht            # App overview
│   ├── basics.cht         # Tutorial
│   ├── bkhowto.cht        # How-to guides
│   ├── bktips.cht         # Tips
│   ├── bktools.cht        # Tool reference
│   ├── errors.cht         # Error messages
│   ├── toolhelp.cht       # Detailed tool docs (734KB)
│   ├── htactors.cht       # Actor database (537KB)
│   ├── htscenes.cht       # Scene library
│   ├── htsounds.cht       # Sound library
│   └── ... (many more)    # Gadgets, projects, easels
│
└── helpaud/               # Help audio
    └── sound/             # Audio for help system
```

---

## /tools/ — Content Pipeline

| Tool | File | Purpose |
|------|------|---------|
| sitobren | sitobren.cpp | SoftImage .hrc → Chunky (actors, actions, backgrounds) |
| tdfmake | tdfmake.cpp | Build 3D font (TDF) chunks from model directories |
| mktmap | mktmap.cpp | Bitmap → texture map (TMAP) |
| pbmtobmp | pbmtobmp.cpp | PBM → MBMP conversion |

---

## /doc/historical/

| File | Content |
|------|---------|
| filetree.txt | Original 1995 file hierarchy for "Socrates" |
| trd.txt | Test Release Document (4/27/95) — feature matrix |
| newtrd.txt | Updated test release variant |
| removed_files.txt | Files removed from original codebase |
