# SYMBOL_MAP.md — 3DMMEx Class and Symbol Reference

Quick lookup for key classes, structs, and functions. Organized by layer.

---

## Engine Layer — Core Domain Classes

| Class | File(s) | Header | Purpose |
|-------|---------|--------|---------|
| MVIE | src/engine/movie.cpp | inc/movie.h | Movie document: playback, scenes, roll call, undo |
| SCEN | src/engine/scene.cpp | inc/scene.h | Scene: frame control, actors, text, sound events |
| ACTR | src/engine/actor.cpp, actredit.cpp, actrsave.cpp, actrsnd.cpp | inc/actor.h | Actor: animation, routes, events, costumes |
| BODY | src/engine/body.cpp | inc/body.h | BRender actor tree: parts, materials, hit detection |
| TMPL | src/engine/tmpl.cpp | inc/tmpl.h | Actor template/species: body structure, actions, costumes |
| ACTN | src/engine/tmpl.cpp | inc/tmpl.h | Action: animation cels, transforms, sounds |
| BKGD | src/engine/bkgd.cpp | inc/bkgd.h | Background: lights, cameras, placement points, palette |
| MODL | src/engine/modl.cpp | inc/modl.h | BRender model wrapper (BMDL) |
| MTRL | src/engine/mtrl.cpp | inc/mtrl.h | Material (color, shader) wrapper (BMTL) |
| CMTL | src/engine/mtrl.cpp | inc/mtrl.h | Custom material per body-part set |
| TBOX | src/engine/tbox.cpp | inc/tbox.h | Text box: styled text overlay |
| TDT | src/engine/tdt.cpp | inc/tdt.h | 3D text object |
| TDF | src/engine/tdf.cpp | inc/tdf.h | 3D font definitions |
| MSND | src/engine/msnd.cpp | inc/msnd.h | Movie sound object |
| TAGM | src/engine/tagman.cpp | inc/tagman.h | Resource tag manager |
| TAGL | src/engine/tagl.cpp | inc/tagl.h | Tag list container |

### Engine Data Structures

| Struct | Header | Purpose |
|--------|--------|---------|
| XYZ | inc/actor.h | 3D point (dxr, dyr, dzr) |
| RPT | inc/actor.h | Route point (XYZ + distance) |
| RTEL | inc/actor.h | Route location (node + offset + frame delta) |
| AEV | inc/actor.h | Actor event (type + frame + location + payload) |
| AEVADD | inc/actor.h | Add-to-stage event |
| AEVACTN | inc/actor.h | Action change event |
| AEVCOST | inc/actor.h | Costume change event |
| AEVSND | inc/actor.h | Sound event |
| CEL | inc/tmpl.h | Animation cel frame |
| CPS | inc/tmpl.h | Cel part spec (model + transform) |
| XFRM | inc/actor.h | Current transformation state |
| BDS | inc/bkgd.h | Background default sound |
| TAG | inc/tagman.h | Resource tag (file + CTG + CNO) |

### Engine Undo Classes

| Class | Header | Purpose |
|-------|--------|---------|
| SUNA | inc/scene.h | Actor add/remove undo |
| SUNC | inc/scene.h | Costume change undo |
| SUNK | inc/scene.h | Action change undo |
| SUNP | inc/scene.h | Path/route change undo |
| SUNS | inc/scene.h | Sound change undo |
| SUNT | inc/scene.h | Text box change undo |

---

## Engine Layer — Key Methods

### MVIE (Movie)
| Method | Purpose |
|--------|---------|
| PmvieNew() | Create new movie |
| FGotoFrm() | Jump to frame |
| FPlayAll() / FStop() | Start/stop playback |
| FAddSnd() / FRemSnd() | Add/remove sounds |
| FInsScen() / FRemScen() | Insert/remove scenes |

### SCEN (Scene)
| Method | Purpose |
|--------|---------|
| PscenNew() / PscenRead() | Create/load scene |
| FGotoFrm() | Advance to frame |
| FAddActr() / FRemActr() | Add/remove actor |
| FSetBkgd() | Set background |
| FAddSnd() | Add frame sound event |

### ACTR (Actor)
| Method | Purpose |
|--------|---------|
| PactrNew() | Create from template |
| FGotoFrame() | Position at frame |
| FSetAction() | Change action/animation |
| FSetCostume() | Change costume |
| FAddOnStage() / FRemFromStage() | Enter/exit scene |
| FRotate() / FScale() / FPull() | Transform actor |
| FBeginRecord() / FRecordMove() / FEndRecord() | Record path |
| FTweakRoute() / FMoveRoute() | Edit path |
| FSetSnd() | Set motion-match sound |

### BODY (Visual)
| Method | Purpose |
|--------|---------|
| PbodyNew() | Create body from part list |
| LocateOrient() | Set position + orientation |
| SetPartModel() | Assign model to body part |
| SetPartSetMtrl() / SetPartSetCmtl() | Assign material |
| FPtInBody() | Mouse hit test |

---

## BRender Layer

| Class | File | Purpose |
|-------|------|---------|
| BWLD | bren/inc/bwld.h | 3D world: scene root, camera, render buffers |
| TMAP | bren/inc/tmap.h | Texture map management |
| ZBMP | bren/inc/zbmp.h | Z-buffer bitmap |
| (types) | bren/inc/bren.h | BACT, BMDL, BMTL, BLIT, BCAM, BPMP, BRS, BRA, BVEC3, BMAT34 |

---

## Studio UI Layer

| Class | File | Header | Purpose |
|-------|------|--------|---------|
| APP | src/studio/utest.cpp | inc/utest.h | App init, resolution, splash, global state |
| STDIO | src/studio/studio.cpp | inc/studio.h | Central studio UI coordinator |
| SMCC | src/studio/studio.cpp | inc/studio.h | Studio-Movie callback bridge |
| BRWD | src/studio/browser.cpp | inc/browser.h | Browser base class |
| BRWL | src/studio/browser.cpp | inc/browser.h | List-based browser (chunky) |
| BRWP | src/studio/browser.cpp | inc/browser.h | Props/Actors browser |
| BRWB | src/studio/browser.cpp | inc/browser.h | Backgrounds browser |
| BRWC | src/studio/browser.cpp | inc/browser.h | Cameras browser |
| BRWN | src/studio/browser.cpp | inc/browser.h | Named list browser |
| BRWM | src/studio/browser.cpp | inc/browser.h | Music browser |
| BRWI | src/studio/browser.cpp | inc/browser.h | Import Sounds browser |
| BRWT | src/studio/browser.cpp | inc/browser.h | Text-based browser |
| BRWA | src/studio/browser.cpp | inc/browser.h | Actions browser |
| BRWR | src/studio/browser.cpp | inc/browser.h | Roll call browser |
| ESL | src/studio/esl.cpp | inc/esl.h | Easel base |
| ESLA | src/studio/esl.cpp | inc/esl.h | Actor easel |
| ESLT | src/studio/esl.cpp | inc/esl.h | Text easel |
| SNE | src/studio/esl.cpp | inc/esl.h | Sound easel |
| ESLL | src/studio/esl.cpp | inc/esl.h | Listener easel |
| ESLR | src/studio/esl.cpp | inc/esl.h | Recording easel |
| APE | src/studio/ape.cpp | inc/ape.h | Actor preview entity |
| MP / MPFNT | src/studio/popup.cpp | inc/popup.h | Popup menus (font, color, size) |
| SSCB | src/studio/utestscb.cpp | inc/stdioscb.h | Scene/frame scrollbars |
| SCRT | src/studio/scnsort.cpp | inc/scnsort.h | Scene sorter |
| TGOB | src/studio/tgob.cpp | inc/tgob.h | Text display GOB |
| SPLOT | src/studio/splot.cpp | inc/splot.h | Splash screen machine |
| TATR | src/studio/tatr.cpp | inc/tatr.h | Theater (playback-only) |

---

## Kauai Framework — Core Classes

### Base & Utilities
| Class | File | Purpose |
|-------|------|---------|
| BASE | kauai/src/base.cpp | Runtime type system, assertions |
| ERS | kauai/src/utilerro.cpp | Error reporting |
| RND / SFL | kauai/src/utilrnd.cpp | Random / shuffle |
| STN | kauai/src/utilstr.cpp | String class |
| RC / PT | kauai/src/utilint.cpp | Rectangle / Point |

### Collections
| Class | File | Purpose |
|-------|------|---------|
| GL / AL | kauai/src/groups.cpp | Generic list / Allocated list |
| GG / AG | kauai/src/groups.cpp | Generic group / Allocated group |
| GST / AST | kauai/src/groups.cpp | Generic string table / Allocated |
| BSM / BSF | kauai/src/stream.cpp | Memory stream / File stream |

### File I/O & Resources
| Class | File | Purpose |
|-------|------|---------|
| FIL | kauai/src/file.cpp | File handle |
| FNI / FNE | kauai/src/fni.cpp | File name interface / enumerator |
| BLCK | kauai/src/file.cpp | File block |
| CFL | kauai/src/chunk.cpp | Chunky file |
| CRF / CRM | kauai/src/crf.cpp | Chunky resource file / manager |
| CODC / KCDC | kauai/src/codec.cpp | Codec / Kauai codec |

### Scripting
| Class | File | Purpose |
|-------|------|---------|
| LEXB | kauai/src/lex.cpp | Lexical analyzer |
| SCCB / SCCG | kauai/src/scrcom.cpp | Script compiler / Kidspace compiler |
| SCPT | kauai/src/screxe.cpp | Script (loaded from file) |
| SCEB / SCEG | kauai/src/screxe.cpp | Script execution / Kidspace extension |
| STRG | kauai/src/screxe.cpp | String registry |
| CHSE | kauai/src/chse.cpp | Chunky source emitter |
| CHCM / CHLX | kauai/src/chcm.cpp | Chunky compiler / lexer |

### Graphics
| Class | File | Purpose |
|-------|------|---------|
| GNV | kauai/src/gfx.cpp | Graphics environment |
| GPT | kauai/src/gfx.cpp | Graphics port |
| NTL / ACR | kauai/src/gfx.cpp | Palette / Color palette |
| MBMP | kauai/src/mbmp.cpp | Masked bitmap |
| PIC | kauai/src/pic.cpp | Vector graphics |
| REGN / REGSC | kauai/src/region.cpp | Region / Region scroll |

### GUI Framework
| Class | File | Purpose |
|-------|------|---------|
| CMH | kauai/src/cmd.cpp | Command handler (event dispatch) |
| CEX | kauai/src/cmd.cpp | Command executor |
| GOB / GTE | kauai/src/gob.cpp | Graphic object / GOB table |
| APPB | kauai/src/appb.cpp | Application base |
| DOCB | kauai/src/docb.cpp | Document base |
| DDG / DMD / DMW | kauai/src/docb.cpp | Document displays |
| UNDB | kauai/src/docb.cpp | Undo base |
| CTL / SCB / WSB | kauai/src/ctl.cpp | Controls / Scrollbars |
| DLG / DIT | kauai/src/dlg.cpp | Dialogs |
| CURS / CURF | kauai/src/cursor.cpp | Cursors |
| MUB | kauai/src/menu*.cpp | Menu base |
| ATBL | kauai/src/accelerator.cpp | Accelerator table |
| CLOK | kauai/src/clok.cpp | Clock/timer |
| CLIP | kauai/src/clip.cpp | Clipboard |

### Kidspace
| Class | File | Purpose |
|-------|------|---------|
| GOK | kauai/src/kidspace.cpp | Kidspace object (script-driven GOB) |
| GOKD / GOKDF | kauai/src/kidworld.cpp | Kidspace object definition |
| WOKS | kauai/src/kidworld.cpp | Kidspace world |
| GKDS | kauai/src/kidworld.cpp | Kidspace scene |
| GORP | kauai/src/kidspace.cpp | Visual representation base |
| GORF / GORB / GORT / GORV | kauai/src/kidspace.cpp | Fill / Bitmap / Tiled / Vector reps |
| HTOP / HBAL / HBTN | kauai/src/kidhelp.cpp | Help topics / balloons / buttons |
| TXHD / TXHG | kauai/src/kidhelp.cpp | Text help display |

### Text & Documents
| Class | File | Purpose |
|-------|------|---------|
| EDCB / EDPL / EDSL / EDML | kauai/src/text.cpp | Text editor controls |
| TXTB / TXPD / TXRD | kauai/src/rtxt.cpp | Text base / Rich text |
| TXDC / TXDD | kauai/src/textdoc.cpp | Plain text document |
| CHP / PAP | kauai/src/rtxt.cpp | Character / Paragraph properties |

### Sound & Media
| Class | File | Purpose |
|-------|------|---------|
| SNDV | kauai/src/sndm.cpp | Sound device interface |
| SNDM / SNDMQ | kauai/src/sndm.cpp | Sound manager / queue |
| MSTP | kauai/src/midi.cpp | MIDI stream player |
| MIDS | kauai/src/midi.cpp | MIDI synth |
| MIDP / MDPS | kauai/src/mididev.cpp | MIDI devices |
| WMS / WMSB | kauai/src/midistreamwin.cpp | Windows MIDI stream |
| FMS | kauai/src/midistreamfluidsynth.cpp | FluidSynth MIDI |
| SDAM | kauai/src/sndam.cpp | AudioMan wave sound |
| MiniaudioManager | kauai/src/sndmamanager.cpp | miniaudio manager |
| MiniaudioStream | kauai/src/sndmastream.cpp | miniaudio stream |
| MiniaudioDevice | kauai/src/sndmadevice.cpp | miniaudio device |
| MiniaudioSoundInstance | kauai/src/sndmasound.cpp | miniaudio sound |
| GVID / GVDS | kauai/src/video*.cpp | Video playback |
| SPLC | kauai/src/spell.cpp | Spell checker |

---

## Build Targets (Executables)

| Target | Type | Platform | Purpose |
|--------|------|----------|---------|
| studio (→ 3dmovie) | Executable | Cross-platform | Main application |
| chomp | Executable | Cross-platform | Chunky file compiler |
| khello | Executable | Cross-platform | Test app |
| KauaiTest | Test | Cross-platform | Core unit tests |
| KauaiGuiTest | Test | Cross-platform | GUI unit tests |
| ched | Executable | Win32 only | Chunky editor |
| chelp | Executable | Win32 only | Help editor |
| ft | Executable | Win32 only | Frame tester |
| ut | Executable | Win32 only | Utility tester |
| mkmbmp | Executable | Win32 only | Bitmap maker |
| kpack | Executable | Win32 only | Resource packer |
| chmerge | Executable | Win32 only | Chunky merger |
| chelpdmp | Executable | Win32 only | Help dumper |

---

## Content Pipeline Tools

| Tool | File | Purpose |
|------|------|---------|
| sitobren | tools/sitobren.cpp | SoftImage .hrc → Chunky actors/backgrounds |
| tdfmake | tools/tdfmake.cpp | Model dirs → TDF (3D font) chunks |
| mktmap | tools/mktmap.cpp | Bitmap → TMAP texture maps |
| pbmtobmp | tools/pbmtobmp.cpp | PBM → MBMP bitmaps |
