# DEPENDENCY_MAP.md — 3DMMEx Dependencies

All external and internal dependencies, build targets, and link relationships.

---

## External Dependencies (FetchContent)

| Dependency | Source | Version | Condition |
|-----------|--------|---------|-----------|
| 3DMM-BRender | github.com/benstone/3DMM-BRender | branch: blazin | 3DMM_BRENDER_LIBRARY=Source |
| AudioManDecomp | github.com/benstone/audioman-decomp | branch: main | 3DMM_AUDIOMAN_LIBRARY=Decompilation (Windows) |
| SDL2 | github.com/libsdl-org/SDL | 2.32.6 | SDL GUI + Windows |
| SDL2_ttf | github.com/libsdl-org/SDL_ttf | 2.24.0 | SDL GUI + Windows |
| miniaudio | github.com/mackron/miniaudio | 0.11.23 | SDL GUI |
| FluidSynth | github.com/FluidSynth/fluidsynth | 2.5.2 | SDL GUI + Windows |
| SoundFont | github.com/musescore/MuseScore | MS Basic.sf3 | SDL GUI |
| iniparser | gitlab.com/iniparser/iniparser | branch: main | SDL GUI |
| nativefiledialog-extended | github.com/btzy/nativefiledialog-extended | latest | SDL GUI |
| Google Test | github.com/google/googletest | v1.15.0 | BUILD_TESTS=ON |

---

## External Dependencies (find_package)

| Package | Required | Condition |
|---------|----------|-----------|
| SDL2 | Yes (CONFIG) | SDL GUI |
| SDL2_ttf | Yes (CONFIG) | SDL GUI |
| FluidSynth | Yes (CONFIG) | SDL GUI |
| PkgConfig | Optional | SDL GUI (for GStreamer) |
| GStreamer 1.0 | Optional | SDL GUI + PkgConfig (video) |
| Fontconfig | Optional | SDL + Linux (font discovery) |
| BRender | Required | 3DMM_BRENDER_LIBRARY=Original |
| AudioMan | Required | 3DMM_AUDIOMAN_LIBRARY=Original |
| ClangTidy | Optional | Static analysis tooling |
| CCache | Optional | Compiler cache |

---

## Windows System Libraries

| Library | Used By | Purpose |
|---------|---------|---------|
| winmm | KauaiBase, KauaiSound | Windows multimedia (MIDI, timers) |
| msacm32 | KauaiSound, studio | Audio Compression Manager |
| Vfw32 | KauaiVideo, studio | Video for Windows |
| mpr | studio | WNetGetUser (network username) |
| Comctl32 | studio | Common Controls 6.0 |

---

## Internal Library Dependency Graph

### Foundation (bottom-up)

```
KauaiBase
    │
    ├── KauaiGroup ─────────────────────────────────────┐
    │       │                                            │
    │       ├── KauaiFile (+ KauaiGroup)                 │
    │       │                                            │
    │       ├── KauaiStream (+ KauaiGroup)               │
    │       │       │                                    │
    │       │       └── KauaiLexer (+ KauaiStream)       │
    │       │                                            │
    │       ├── KauaiChse (+ KauaiGroup)                 │
    │       │                                            │
    │       └── KauaiSpell (+ KauaiBase, KauaiGroup)     │
    │                                                    │
    ├── KauaiPic (+ KauaiBase)                           │
    │                                                    │
    └── KauaiMbmpIO (+ KauaiFile)                        │
                                                         │
```

### Scripting

```
KauaiLexer
    │
    └── KauaiScrCom (+ KauaiLexer, KauaiScrExe)
            │
            └── KauaiKidCom (+ KauaiScrCom)

KauaiFile + KauaiGroup
    │
    └── KauaiScrExe

KauaiLexer + KauaiChse + KauaiMbmpIO + KauaiKidCom
    │
    └── KauaiChcm (chunky compiler)
```

### Sound & Media

```
KauaiGroup + KauaiStream
    │
    └── KauaiSound
            ├── [Win32] AudioMan + msacm32 + Vfw32 + winmm
            └── [SDL]   miniaudio + FluidSynth
```

### GUI

```
KauaiBase + KauaiGroup + KauaiMbmpIO + KauaiSound + KauaiVideo + KauaiPic
    │
    └── KauaiGui
            │
            ├── KauaiGuiMain (+ platform entry point)
            ├── KauaiDlg (+ platform dialogs)
            ├── KauaiCtl
            │       │
            │       └── KauaiDoc (+ KauaiGui, KauaiCtl)
            │               │
            │               ├── KauaiRichText (+ KauaiDoc, KauaiStream)
            │               └── KauaiPlainText (+ KauaiDoc, KauaiStream)
            │
            ├── KauaiTextEdit (+ KauaiGui, KauaiStream)
            │
            └── KauaiKid (+ KauaiGui, KauaiScrExe, KauaiScrCom, KauaiTextEdit)

KauaiGui
    │
    └── KauaiVideo
            ├── [Win32] Vfw32
            └── [SDL]   GStreamer (optional)
```

### Application

```
KauaiBase + KauaiGui + KauaiDoc + KauaiRichText
    │
    └── bren (+ BRender::Libraries)
            │
            └── engine (+ bren, KauaiBase, KauaiGui, KauaiDoc, KauaiRichText)
                    │   [+ AudioMan if enabled]
                    │   [+ miniaudio if SDL]
                    │
                    └── studio (executable)
                            ├── engine
                            ├── KauaiBase, KauaiGui, KauaiGuiMain
                            ├── KauaiKid, KauaiDlg
                            ├── [Win32] mpr, vfw32, msacm32
                            ├── [SDL] iniparser-static
                            └── [SDL + file dialog] nfd::nfd
```

---

## Build Target Summary

### Libraries (26 Kauai + 2 App)

| # | Target | Type |
|---|--------|------|
| 1 | KauaiBase | STATIC |
| 2 | KauaiGroup | STATIC |
| 3 | KauaiFile | STATIC |
| 4 | KauaiStream | STATIC |
| 5 | KauaiLexer | STATIC |
| 6 | KauaiScrExe | STATIC |
| 7 | KauaiScrCom | STATIC |
| 8 | KauaiKidCom | STATIC |
| 9 | KauaiMbmpIO | STATIC |
| 10 | KauaiChse | STATIC |
| 11 | KauaiChcm | STATIC |
| 12 | KauaiPic | STATIC |
| 13 | KauaiSpell | STATIC |
| 14 | KauaiSound | STATIC |
| 15 | KauaiGui | STATIC |
| 16 | KauaiGuiMain | STATIC |
| 17 | KauaiVideo | STATIC |
| 18 | KauaiDoc | STATIC |
| 19 | KauaiCtl | STATIC |
| 20 | KauaiDlg | STATIC |
| 21 | KauaiRichText | STATIC |
| 22 | KauaiTextEdit | STATIC |
| 23 | KauaiPlainText | STATIC |
| 24 | KauaiKid | STATIC |
| 25 | KauaiTestLib | STATIC |
| 26 | FDivStub | STATIC (Win32 + Original AudioMan) |
| 27 | bren | STATIC |
| 28 | engine | STATIC |

### Executables

| Target | Output Name | Platform |
|--------|-------------|----------|
| studio | 3dmovie | Cross-platform |
| chomp | chomp | Cross-platform |
| khello | khello | Cross-platform |
| KauaiTest | KauaiTest | Cross-platform (test) |
| KauaiGuiTest | KauaiGuiTest | Cross-platform (test) |
| ched | ched | Win32 only |
| chelp | chelp | Win32 only |
| ft | ft | Win32 only |
| ut | ut | Cross-platform |
| mkmbmp | mkmbmp | Win32 only |
| kpack | kpack | Win32 only |
| chmerge | chmerge | Win32 only |
| chelpdmp | chelpdmp | Win32 only |

---

## Conditional Compilation Defines

| Macro | Set When | Scope |
|-------|----------|-------|
| WIN | Windows platform | Global |
| DEBUG | Debug configuration | Global |
| IN_80386 | ENABLE_ASM_X86=ON | Global |
| LITTLE_ENDIAN | Little-endian CPU | Global |
| UNICODE / _UNICODE | 3DMM_UNICODE=ON | Global |
| HAS_AUDIOMAN | AudioMan enabled | Global |
| KAUAI_WIN32 | 3DMM_GUI=Win32 | KauaiBase (PUBLIC) |
| KAUAI_SDL | 3DMM_GUI=SDL | KauaiBase (PUBLIC) |
| _LPCVOID_DEFINED | Always | KauaiBase (PUBLIC) |
| STRICT | Always | KauaiBase (PUBLIC) |
| NAMES | Always | studio (PRIVATE) |

---

## Build Configuration Options

| Option | Default | Values | Purpose |
|--------|---------|--------|---------|
| 3DMM_GUI | Win32 (Windows) / SDL (other) | Win32, SDL | GUI backend |
| 3DMM_BRENDER_LIBRARY | Original (MSVC x86) / Source (other) | Original, Source | BRender library source |
| 3DMM_AUDIOMAN_LIBRARY | Decompilation (Windows) / None (other) | Original, Decompilation, None | AudioMan library |
| 3DMM_UNICODE | OFF | ON/OFF | Unicode support |
| ENABLE_ASM_X86 | OFF | ON/OFF | Original x86 assembly |
| BUILD_TESTS | ON | ON/OFF | Build test suite |
| BUILD_PACKAGES | ON | ON/OFF | Package generation |
| 3DMM_PACKAGE_WIX | OFF (auto) | ON/OFF | MSI package via WiX |
| 3DMM_PACKAGE_ZIP | ON | ON/OFF | Portable ZIP package |
| 3DMM_PRECOMPILED_CHUNKY_PATH | "" | Path | Skip chunky compilation |

---

## Build Presets (CMakePresets.json)

### Windows MSVC
- x86-msvc-debug, x86-msvc-release, x86-msvc-relwithdebinfo, x86-msvc-minsizerel
- x64-msvc-debug, x64-msvc-release, x64-msvc-relwithdebinfo, x64-msvc-minsizerel
- arm64-msvc-debug, arm64-msvc-relwithdebinfo

### Windows Clang
- x86-clangcl-debug, x86-clangcl-relwithdebinfo
- x64-clangcl-debug, x64-clangcl-relwithdebinfo

### SDL Variants
- sdl-x64-msvc-debug
- sdl-x64-clangcl-debug
- sdl-x86_64-gcc-linux-debug, sdl-x86_64-gcc-linux-release

### All presets use:
- Generator: Ninja
- Build dir: `build/{presetName}`
- Install dir: `dist/{presetName}`
