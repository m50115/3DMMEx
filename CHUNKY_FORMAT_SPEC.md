# CHUNKY_FORMAT_SPEC.md — Binary File Format Specification

This is the exact binary specification for 3D Movie Maker file formats. Any reimplementation MUST produce bit-perfect output compatible with the original 1995 release.

---

## 1. CFL (Chunky File) — Top-Level Structure

```
[CFP Header — 128 bytes]
[Chunk Data (heap) — variable]
[GG Index (serialized) — at fpIndex, cbIndex bytes]
[FSM Free Map (optional) — at fpMap, cbMap bytes]
```

---

## 2. CFP (Chunky File Prefix) — 128 bytes

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | lwMagic | Magic number: `0x43484E32` ('CHN2' LE) or `0x324E4843` (BE) |
| 4 | 4 | ctgCreator | Creator chunk tag (e.g. `'SOC '` for 3DMM) |
| 8 | 2 | dver.swCur | Current version number |
| 10 | 2 | dver.swBack | Backward-compatible version number |
| 12 | 2 | bo | Byte order: `0x0001` (native) or `0x0100` (swapped) |
| 14 | 2 | osk | OS kind: Mac or Windows |
| 16 | 4 | fpMac | Logical end of file position |
| 20 | 4 | fpIndex | File position of chunk index |
| 24 | 4 | cbIndex | Size of chunk index in bytes |
| 28 | 4 | fpMap | File position of free space map |
| 32 | 4 | cbMap | Size of free space map |
| 36 | 92 | rglwReserved[23] | Reserved (must be zero) |

**BOM:** `0xB55FFC00`
**Magic constant:** `klwMagicChunky = 0x43484E32`

### Version History

| Version | Feature |
|---------|---------|
| 1 | Original format |
| 2 | Compression support |
| 3 | STN format chunk names |
| 4 | Compact index (CRPSM) |
| 5 | Forest support (fcrpForest flag) |

Current: `kcvnCur = 5`, Backward: `kcvnBack = 4`, Minimum readable: `kcvnMin = 1`

---

## 3. Chunk Index

The index is a serialized GG (General Group) of CRP entries.

### CRP — Big Index Format (CRPBG) — 32 bytes

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | cki.ctg | Chunk type tag (uint32) |
| 4 | 4 | cki.cno | Chunk number (uint32) |
| 8 | 4 | fp | File position of chunk data |
| 12 | 4 | cb | Data size in bytes |
| 16 | 4 | ckid | Number of child chunks |
| 20 | 4 | ccrpRef | Parent reference count |
| 24 | 4 | rti | Run-time ID |
| 28 | 4 | grfcrp | Flags (see below) |

**BOM:** `0xFFFF0000` (grfcrp format) or `0xFFFE0000` (byte format)

### CRP — Small Index Format (CRPSM) — 20 bytes

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | cki.ctg | Chunk type tag |
| 4 | 4 | cki.cno | Chunk number |
| 8 | 4 | fp | File position |
| 12 | 4 | luGrfcrpCb | Packed: low byte = grfcrp, upper 3 bytes = cb |
| 16 | 2 | ckid | Child count |
| 18 | 2 | ccrpRef | Parent ref count |

**BOM:** `0xFF500000`
**Packing:** `grfcrp = luGrfcrpCb & 0xFF`, `cb = luGrfcrpCb >> 8` (max `0xFFFFFF`)

### CRP Flags (grfcrp)

| Flag | Value | Meaning |
|------|-------|---------|
| fcrpNil | 0x00 | No flags |
| fcrpOnExtra | 0x01 | Data on extra file |
| fcrpLoner | 0x02 | Chunk can exist without parent |
| fcrpPacked | 0x04 | Data is compressed |
| fcrpMarkT | 0x08 | Validation mark |
| fcrpForest | 0x10 | Chunk contains embedded forest |

### Variable Data Per CRP (in GG variable portion)

For each CRP entry, the GG variable data contains:
1. `KID kids[ckid]` — Array of child references
2. `STN name` — Optional chunk name (serialized STN format)

---

## 4. CKI (Chunk Identifier) — 8 bytes

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | ctg — Chunk type (uint32, typically 4-char ASCII) |
| 4 | 4 | cno — Chunk number (uint32) |

**BOM:** `0xF0000000`

---

## 5. KID (Child Reference) — 12 bytes

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | cki.ctg — Child chunk type |
| 4 | 4 | cki.cno — Child chunk number |
| 8 | 4 | chid — Child ID (relationship identifier) |

**BOM:** `0xFC000000`

---

## 6. FSM (Free Space Map Entry) — 8 bytes

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | fp — File position of free block |
| 4 | 4 | cb — Size of free block |

**BOM:** `0xF0000000`

---

## 7. Collection Serialization Formats

### GL (General List) — Fixed-size elements

```
[int32_t cbEntry]     // Bytes per element
[int32_t ivMac]       // Number of elements
[int16_t bo]          // Byte order
[int16_t osk]         // OS kind
[data: ivMac * cbEntry bytes]
```

### AL (Allocated List) — Same as GL + free bitmap

```
[GL header + data]
[free bitmap: ceil(ivMac / 8) bytes]
```

### GG (General Group) — Fixed + variable-size elements

```
[int32_t cbFixed]     // Fixed portion size per element
[int32_t ivMac]       // Number of elements
[int16_t bo]          // Byte order
[int16_t osk]         // OS kind
[fixed data: ivMac * cbFixed bytes]
[LOC array: ivMac * 8 bytes]  // {int32_t bv, int32_t cb} per element
[variable data: bvMac bytes]
```

### AG (Allocated Group) — Same as GG + free bitmap

```
[GG header + data]
[free bitmap: ceil(ivMac / 8) bytes]
```

### GST (String Table)

```
[int32_t cbExtra]     // Extra data per string entry
[int32_t istnMac]     // Number of strings
[int16_t bo]          // Byte order
[int16_t osk]         // OS kind
[entries: istnMac * (4 + cbExtra) bytes]  // {int32_t bst, extra[cbExtra]}
[string data: total bytes]
```

### AST (Allocated String Table) — Same as GST + free bitmap

---

## 8. Compression Format

### Header — 8 bytes (prepended to compressed data)

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | cfmt — Format identifier |
| 4 | 4 | cbDecompressed — Original uncompressed size |

### Format Identifiers

| ID | Value | Name |
|----|-------|------|
| kcfmtKauai | `0x4B434443` ('KCDC') | Kauai Codec |
| kcfmtKauai2 | `0x4B434432` ('KCD2') | Kauai Codec 2 |
| cfmtNil | `0x00000000` | No compression |

### KCDC Algorithm — LZ77 with bit-packing

Decompression loop:
```
while (not done):
    if next_bit == 0:
        output literal byte (next 8 bits)
        continue

    // Match found — determine offset
    if next_bit == 1:
        offset = next_6_bits + 0x01              // range: 0x01–0x40
    elif next_bit == 1:
        offset = next_9_bits + 0x41              // range: 0x41–0x240
    elif next_bit == 1:
        offset = next_12_bits + 0x241            // range: 0x241–0x1240
    else:
        offset = next_20_bits
        if offset == 0: DONE (terminator)
        offset += 0x1241                          // range: 0x1241–0x101240

    // Determine match length (logarithmic encoding)
    length = decode_length()  // minimum 2

    // Copy from history buffer
    copy length bytes from (output - offset) to output
```

**Minimum tail padding:** 6 bytes of `0xFF` at end of compressed data

---

## 9. Movie File Format (.3mm)

### Chunk Hierarchy

```
MVIE (root, cno=1 typically)
 ├── SCEN (child, chid=0) — Scene 0
 │    ├── ACTR (child) — Actor
 │    │    ├── PATH (child, chid=0) — GL of route points
 │    │    └── GGAE (child, chid=0) — GG of actor events
 │    ├── TBOX (child) — Text box
 │    └── (frame events, background refs, etc.)
 ├── SCEN (child, chid=1) — Scene 1
 ├── ... more scenes
 ├── GST (child, chid=kchidGstMactr) — Actor roll call
 ├── GST (child, chid=kchidGstSource) — Source info
 └── MSND (children) — Sounds
```

### MFP (Movie File Prefix) — 8 bytes

| Offset | Size | Field |
|--------|------|-------|
| 0 | 2 | bo — Byte order |
| 2 | 2 | osk — OS kind |
| 4 | 2 | dver.swCur — Current version (= 2) |
| 6 | 2 | dver.swBack — Backward version (= 2) |

**BOM:** `0x55000000`

### MACTR (Actor Roll Call Entry) — in GST extra data

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | arid — Actor ID |
| 4 | 4 | cactRef — Reference count |
| 8 | 4 | grfbrws — Browser properties |
| 12 | 12 | tagTmpl — Template TAG (see TAG format) |

**BOM:** `0xFC000000 | (kbomTag >> 4)`

---

## 10. Actor Serialization

### ACTF (Actor on File) — 44 bytes

| Offset | Size | Field |
|--------|------|-------|
| 0 | 2 | bo — Byte order |
| 2 | 2 | osk — OS kind |
| 4 | 4 | dxyzFullRte.dxr — Route translation X (BRS fixed-point) |
| 8 | 4 | dxyzFullRte.dyr — Route translation Y |
| 12 | 4 | dxyzFullRte.dzr — Route translation Z |
| 16 | 4 | arid — Unique actor ID |
| 20 | 4 | nfrmFirst — First frame number |
| 24 | 4 | nfrmLast — Last frame number |
| 28 | 12 | tagTmpl — Template TAG (serialized TAGF) |
| 40 | 4 | (padding/alignment) |

**BOM:** `0x5FFC0000 | kbomTag`

### Actor Event Types

| Type | Value | Variable Data |
|------|-------|---------------|
| aetAdd | 0 | AEVADD: translation + orientation per subroute |
| aetActn | 1 | AEVACTN: action ID + starting cel |
| aetCost | 2 | AEVCOST: body part set + texture tag |
| aetSnd | 3 | AEVSND: loop, volume, cel, motion match |
| aetRotF | 4 | Rotation (forward) |
| aetRotH | 5 | Rotation (here) |
| aetSize | 6 | Scale |
| aetPull | 7 | Squash/stretch (3-axis scale) |
| aetFreeze | 8 | Freeze |
| aetStep | 9 | Step size |
| aetMove | 10 | Move/reposition |
| aetTweak | 11 | Tweak path |
| aetRem | 12 | Remove from stage |

---

## 11. Embedded Forest (ECDF) — 24 bytes

| Offset | Size | Field |
|--------|------|-------|
| 0 | 2 | bo — Byte order |
| 2 | 2 | osk — OS kind |
| 4 | 4 | ctg — Chunk type |
| 8 | 4 | chid — Child ID |
| 12 | 4 | cb — Data size |
| 16 | 4 | ckid — Number of children |
| 20 | 4 | grfcrp — Flags |

**BOM:** `0x5FFC0000`

### Forest Grammar

```
Forest → ε | ChunkTree Forest
ChunkTree → ECDF(n, 0) Data(n)                          // Leaf
ChunkTree → ECDF(n, m) Data(n) ChunkTree₁ ... ChunkTreeₘ  // Branch with m children
```

---

## 12. TAG (Resource Reference) Format

### In-Memory TAG

```c
struct TAG {
    int32_t sid;    // Source ID (file reference)
    CTG ctg;        // Chunk type (uint32)
    CNO cno;        // Chunk number (uint32)
};
```

### On-Disk TAGF (Serialized TAG) — 12 bytes

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | sid — Source/file ID |
| 4 | 4 | ctg — Chunk type |
| 8 | 4 | cno — Chunk number |

---

## 13. BRender Model Face — 32 bytes

| Offset | Size | Field |
|--------|------|-------|
| 0 | 2 | vertices[0] |
| 2 | 2 | vertices[1] |
| 4 | 2 | vertices[2] |
| 6 | 2 | edges[0] |
| 8 | 2 | edges[1] |
| 10 | 2 | edges[2] |
| 12 | 4 | material — Material index |
| 16 | 2 | smoothing — Smoothing group |
| 18 | 1 | flags — Edge visibility (bits 0-2) |
| 19 | 1 | _pad0 |
| 20 | 4 | n.x — Plane normal X (br_scalar, fixed-point) |
| 24 | 4 | n.y — Plane normal Y |
| 28 | 4 | n.z — Plane normal Z |
| 32 | 4 | d — Plane offset |

**Total:** 36 bytes (with d field) or 32 bytes (without d, verify in context)

---

## 14. Byte Order Machinery

### BOM Encoding

A BOM (Byte Order Map) is a uint32 bitmask that describes struct field layout. Read 2 bits at a time from MSB:

| Bits | Meaning |
|------|---------|
| 00 | End of fields |
| 01 | int16_t (2 bytes) — swap 2 bytes |
| 10 | int32_t (4 bytes) — swap 4 bytes |
| 11 | int32_t (4 bytes) — swap 4 bytes |

`SwapBytesBom(data, bom)` iterates the BOM and swaps fields accordingly.

### Byte Order Constants

```
kboCur   = 0x0001  // Current/native byte order
kboOther = 0x0100  // Opposite byte order
```

### OS Kind Constants

```
koskMac = 0x6D61  // 'ma' — Macintosh
koskWin = 0x7769  // 'wi' — Windows
```

---

## 15. Fixed-Point Math (BRender)

| Type | C Type | Description |
|------|--------|-------------|
| BRS (br_scalar) | int32_t | 16.16 fixed-point |
| BRA (br_angle) | uint16_t | 0–65535 maps to 0°–360° |
| BVEC3 | BRS[3] | 3D vector |
| BMAT34 | BRS[3][4] | 3x4 transformation matrix |

**Conversion:** `float_value = BRS_value / 65536.0`

---

## 16. Critical Compatibility Notes

1. **Byte order**: Files can be little-endian (Windows) or big-endian (Mac). The `bo` field in every struct header indicates byte order. Reader must swap if `bo == kboOther`.

2. **OS kind**: The `osk` field tracks origin platform. Reader should handle both `koskMac` and `koskWin`.

3. **Index format**: Files version ≥ 4 may use CRPSM (20 bytes). Older files use CRPBG (32 bytes). Reader must handle both.

4. **Compression**: Any chunk with `fcrpPacked` flag uses KCDC or KCD2 compression. The 8-byte header precedes compressed data.

5. **Forest**: Chunks with `fcrpForest` flag contain embedded sub-chunks in forest format (recursive ECDF structures).

6. **Fixed-point**: All 3D coordinates use BRender 16.16 fixed-point. Do NOT convert to float during save — preserve exact values.

7. **String format (STN)**: Chunk names use Kauai STN serialization. Format depends on Unicode mode (rare — most files are ANSI).

8. **Alignment**: No padding between index entries. Chunk data may start at any file offset.

9. **Creator tag**: 3DMM files use `'SOC '` as ctgCreator (internal name "Socrates").

10. **Round-trip safety**: When reading and writing a file, all chunks not modified must be written back byte-identical. Do not re-encode compression, re-order index, or normalize byte order unless the chunk was actually changed.
