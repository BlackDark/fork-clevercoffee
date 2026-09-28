#!/usr/bin/env python3
"""Extract the ten profont/fub glyph atlases out of the installed U8g2 source.

R2-10 (R1-04 step 2). Reads `u8g2_fonts.c` from the U8g2 that the firmware
actually links (`.pio/libdeps/<env>/U8g2/src/clib/u8g2_fonts.c`) and writes

    crates/cc-display/src/font/data.rs

Three modes:

    extract_fonts.py extract   regenerate data.rs from the U8g2 source
    extract_fonts.py check     fail if data.rs is not byte-identical to what
                               extraction would produce (CI guard)
    extract_fonts.py report    print the per-font flash-cost table

WHY THE RLE IS EMBEDDED VERBATIM
--------------------------------
The obvious "convert to a Rust-embedded bitmap format" is `embedded-graphics`'
`ImageRaw`, i.e. an *uncompressed* 1bpp atlas. That is strictly worse here and
`report` prints the numbers: the U8g2 RLE stream is already a per-glyph
run-length encoding, and the C firmware pays for exactly those bytes. Expanding
to a fixed-cell atlas multiplies the flash cost several-fold, and 07-image-size-budget
is a binding constraint on this project. So the converter embeds the RLE stream
byte-for-byte and `src/font/mod.rs` ports the ~120-line U8g2 decoder
(`u8g2_font.c`), which is also the only way to get *exact* pixel parity -- the
glyph placement, the x/y offsets and the delta-x advances all come out of the
same decoder, so `getStrWidth` and `drawStr` agree with the C++ by construction.

The uncompressed figure is still reported, because it is the number a reviewer
needs in order to check this decision.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# The ten fonts the C++ firmware uses. Exactly the set named in R1-04 step 2
# and in the R2-10 task brief; no more, no less.
#
#   profont10/11/12/15/17/22_tf   (6)
#   fub17/20/25/30_tf             (4)
FONTS = [
    "u8g2_font_profont10_tf",
    "u8g2_font_profont11_tf",
    "u8g2_font_profont12_tf",
    "u8g2_font_profont15_tf",
    "u8g2_font_profont17_tf",
    "u8g2_font_profont22_tf",
    "u8g2_font_fub17_tf",
    "u8g2_font_fub20_tf",
    "u8g2_font_fub25_tf",
    "u8g2_font_fub30_tf",
]

# U8G2_FONT_DATA_STRUCT_SIZE -- u8g2_font.c:39.
FONT_INFO_LEN = 23

_DECL = re.compile(
    r"const\s+uint8_t\s+(\w+)\s*\[\s*(\d+)\s*\]\s*U8G2_FONT_SECTION\([^)]*\)\s*=\s*",
    re.MULTILINE,
)
# Simple single-character C escapes. The numeric escapes (`\0`..`\7`) are
# deliberately NOT here: they are prefixes of C's up-to-three-digit octal
# escape, and must be handled by the octal branch in `decode_c_string`.
# Putting them in this table shadows that branch and silently turns `\340`
# into four bytes.
_ESCAPES = {
    "a": 0x0A, "b": 0x0B, "t": 0x09, "n": 0x0A, "v": 0x0B, "f": 0x0C,
    "r": 0x0D, "e": 0x1B, '"': 0x22, "'": 0x27, "?": 0x3F, "\\": 0x5C,
}


def decode_c_string(literal: str) -> bytes:
    """Decode one C string literal body (no surrounding quotes)."""
    out = bytearray()
    i = 0
    while i < len(literal):
        ch = literal[i]
        if ch != "\\":
            code = ord(ch)
            if code > 0x7F:
                raise ValueError(f"non-ASCII byte {ch!r} in a C string literal")
            out.append(code)
            i += 1
            continue
        i += 1
        if i >= len(literal):
            raise ValueError("trailing backslash in a C string literal")
        esc = literal[i]
        if esc in _ESCAPES:
            out.append(_ESCAPES[esc])
            i += 1
        elif esc == "x":
            i += 1
            start = i
            while i < len(literal) and literal[i] in "0123456789abcdefABCDEF":
                i += 1
            if start == i:
                raise ValueError("\\x with no hex digits")
            out.append(int(literal[start:i], 16) & 0xFF)
        elif esc.isdigit():
            start = i
            while i < len(literal) and literal[i] in "01234567" and i - start < 3:
                i += 1
            out.append(int(literal[start:i], 8) & 0xFF)
        else:
            raise ValueError(f"unhandled escape \\{esc}")
    return bytes(out)


def scan_literals(text: str, start: int):
    """Yield the body of every `"..."` at/after `start`, honouring escapes.

    Hand-rolled rather than a regex, for two reasons:

    * the literals mix octal/hex escapes with unescaped printable characters
      (`\\31;` is a two-byte sequence), so no single regex handles them; and
    * the C array body legitimately contains raw `;` and `,` characters
      *inside* a string literal, so the array cannot be delimited by searching
      for the first `;` -- only by scanning quotes.

    Yields `(literal_body, index_just_past_the_closing_quote)`.
    """
    i = start
    n = len(text)
    while i < n:
        if text[i] != '"':
            i += 1
            continue
        i += 1  # opening quote
        begin = i
        while i < n:
            if text[i] == "\\":
                i += 2
                continue
            if text[i] == '"':
                yield text[begin:i], i + 1
                i += 1
                break
            i += 1
        else:
            raise ValueError("unterminated C string literal")


def parse_fonts_c(text: str) -> dict[str, bytes]:
    """Map font symbol name -> raw font bytes, for every font in the file."""
    found: dict[str, bytes] = {}
    for match in _DECL.finditer(text):
        name, declared_len = match.group(1), int(match.group(2))
        data = bytearray()
        literals = scan_literals(text, match.end())
        pos = match.end()
        while True:
            literal, next_pos = next(literals)
            data += decode_c_string(literal)
            pos = next_pos
            # The declaration ends at the first `;` outside a string literal.
            # A raw `;` may appear *inside* a literal (`\31;`), so the test is
            # "does a `;` come before the next opening quote?".
            semi = text.find(";", pos)
            quote = text.find('"', pos)
            if semi != -1 and (quote == -1 or semi < quote):
                break
        # The array is `const uint8_t x[N] = "..."`, so N counts the string
        # literal's implicit NUL terminator -- verified against the compiler,
        # not assumed: `sizeof(u8g2_font_profont11_tf)` is 2251 while the
        # decoded escape sequence is 2250 bytes, and the last stored byte is
        # 0x00. The terminator is kept so the embedded array is byte-identical
        # to the C one (U8g2's own `u8g2_GetFontSize` relies on it).
        if len(data) + 1 != declared_len:
            raise ValueError(
                f"{name}: declared [{declared_len}] bytes (incl. the string's "
                f"implicit NUL), decoded {len(data)}"
            )
        data.append(0x00)
        found[name] = bytes(data)
    return found


# --------------------------------------------------------------------------
# Font decoding -- a direct transcription of u8g2_font.c, used only to compute
# the atlas size for the report and to sanity-check the data we embed.
# --------------------------------------------------------------------------


class BitReader:
    """`u8g2_font_decode_t`'s LSB-first bit cursor (u8g2_font.c:249)."""

    def __init__(self, data: bytes, offset: int) -> None:
        self.data = data
        self.pos = offset
        self.bit = 0

    def unsigned(self, count: int) -> int:
        val = self.data[self.pos] >> self.bit
        total = self.bit + count
        if total >= 8:
            self.pos += 1
            val |= self.data[self.pos] << (8 - self.bit)
            total -= 8
        val &= (1 << count) - 1
        self.bit = total
        return val

    def signed(self, count: int) -> int:
        v = self.unsigned(count)
        return v - (1 << (count - 1))


def font_info(data: bytes) -> dict[str, int]:
    info = {
        "glyph_cnt": data[0],
        "bbx_mode": data[1],
        "bits_per_0": data[2],
        "bits_per_1": data[3],
        "bits_per_char_width": data[4],
        "bits_per_char_height": data[5],
        "bits_per_char_x": data[6],
        "bits_per_char_y": data[7],
        "bits_per_delta_x": data[8],
        "max_char_width": data[9],
        "max_char_height": data[10],
        "x_offset": data[11],
        "y_offset": data[12],
        "ascent_a": data[13],
        "descent_g": data[14],
        "ascent_para": data[15],
        "descent_para": data[16],
        "start_pos_upper_a": (data[17] << 8) | data[18],
        "start_pos_lower_a": (data[19] << 8) | data[20],
        "start_pos_unicode": (data[21] << 8) | data[22],
    }
    return info


def glyph_table(data: bytes):
    """Yield (encoding, glyph_offset) for every glyph, in table order.

    Transcribes `u8g2_font_get_glyph_data`'s walk (u8g2_font.c:759) without the
    binary search shortcut, so the ASCII and the Unicode sections are both seen.
    """
    info = font_info(data)
    end = FONT_INFO_LEN
    # 8-bit section, split into <= 'a', 'A'..'Z', and the rest.
    out = []
    for lower in (False, True):
        pos = FONT_INFO_LEN + (info["start_pos_lower_a"] if lower else info["start_pos_upper_a"] if not lower else 0)
        if lower:
            pos = FONT_INFO_LEN + info["start_pos_lower_a"]
        while True:
            if data[pos + 1] == 0:
                break
            out.append((data[pos], pos + 2))
            pos += data[pos + 1]
    pos = FONT_INFO_LEN + info["start_pos_unicode"]
    pos += (data[pos] << 8) | data[pos + 1]  # skip the unicode index table
    while True:
        e = (data[pos] << 8) | data[pos + 1]
        if e == 0:
            break
        out.append((e, pos + 3))
        pos += data[pos + 2]
    del end
    return out


def atlas_size(data: bytes) -> tuple[int, int, int]:
    """(glyph_count, uncompressed 1bpp atlas bytes, populated pixel count).

    The atlas is the naive fixed-cell form an `ImageRaw` carrier would hold:
    `glyph_cnt` cells of `max_char_width` x `max_char_height`, 1 bit per pixel,
    MSB first, no run-length coding and no shared-glyph deduplication.
    """
    info = font_info(data)
    cell_rows = (info["max_char_width"] + 7) // 8
    cell_bytes = cell_rows * info["max_char_height"]
    table = glyph_table(data)

    populated = 0
    for _enc, off in table:
        r = BitReader(data, off)
        glyph_w = r.unsigned(info["bits_per_char_width"])
        glyph_h = r.unsigned(info["bits_per_char_height"])
        r.signed(info["bits_per_char_x"])
        r.signed(info["bits_per_char_y"])
        r.signed(info["bits_per_delta_x"])
        if glyph_w == 0:
            continue
        # Replay the RLE run-length walk and count set pixels.
        lx = ly = 0
        while ly < glyph_h:
            a = r.unsigned(info["bits_per_0"])
            b = r.unsigned(info["bits_per_1"])
            for run_len, is_fg in ((a, False), (b, True)):
                cnt = run_len
                while True:
                    rem = glyph_w - lx
                    current = rem if cnt >= rem else cnt
                    if is_fg:
                        populated += current
                    if cnt < rem:
                        break
                    cnt -= rem
                    lx = 0
                    ly += 1
                lx += cnt
            if r.unsigned(1) == 0:
                break
    return len(table), info["glyph_cnt"] * cell_bytes, populated


# --------------------------------------------------------------------------
# Rust emission
# --------------------------------------------------------------------------

HEADER = """\
// @generated by crates/cc-display/tools/extract_fonts.py -- DO NOT EDIT BY HAND.
//
// Source of truth: the `u8g2_fonts.c` that the C++ firmware links, i.e.
// `U8g2` 2.36.18 (`platformio.ini`, `lib_deps: olikraus/U8g2 @ 2.36.18`).
// Regenerate with `crates/cc-display/tools/run.sh`; `run.sh --check` is the
// CI guard that fails when this file has drifted from U8g2.
//
// WHY THE BYTES BELOW ARE THE RAW U8g2 RLE STREAM, NOT AN UNCOMPRESSED ATLAS
// ------------------------------------------------------------------------
// R1-04 step 2 asks for a "Rust-embedded bitmap format" and suggests
// `embedded-graphics`' `ImageRaw`. Measured, that is a large flash regression:
// see `FontSize` in this crate's module docs and the per-font table printed by
// `extract_fonts.py report`. The U8g2 encoding is already a per-glyph
// run-length code, the C++ firmware pays for exactly these bytes, and the
// decoder that `src/font/mod.rs` ports is the only thing that yields the
// glyph placement / delta-x advances that `getStrWidth` and `drawStr` must
// agree on for pixel parity. Embedding the stream verbatim is therefore both
// the smaller and the more faithful option; it is a measured decision, not a
// shortcut, and the uncompressed figures are reported so it can be checked.

// Every array below is a U8g2 font in the library's own RLE encoding.
//
// The 23-byte header is `u8g2_font.c:44-68` (`u8g2_read_font_info`); the rest is
// the glyph table, and `FONT_INFO_LEN` is the offset the decoder starts walking
// from. These are `&'static [u8]`, so they land in `.rodata` -- exactly where
// the C++ linker put the same arrays, which is why the flash cost is unchanged.

/// Length of the U8g2 font header, in bytes.
///
/// `u8g2_font.c:39`: `#define U8G2_FONT_DATA_STRUCT_SIZE 23`.
pub const FONT_INFO_LEN: usize = 23;
"""

FONT_DOC = """/// `{name}` -- {glyphs} glyphs, `max_char_width` {w} x `max_char_height` {h} px.
///
/// `ascent_A` {ascent} / `descent_g` {descent} (signed, as `u8g2_font_info_t`
/// declares them `int8_t`, `u8g2.h:253-254`).
///
/// {rle} bytes of RLE ({atlas} bytes as an uncompressed 1bpp `ImageRaw`
/// atlas: {ratio:.1f}x larger).
"""


def s8(value: int) -> int:
    """Reinterpret an unsigned header byte as the `int8_t` U8g2 stores it as."""
    return value - 256 if value >= 128 else value


def rust_array(name: str, data: bytes, per_line: int = 16) -> str:
    lines = [f"pub static {name}: [u8; {len(data)}] = ["]
    for i in range(0, len(data), per_line):
        chunk = data[i: i + per_line]
        lines.append("    " + " ".join(f"0x{b:02x}," for b in chunk))
    lines.append("];")
    return "\n".join(lines)


def emit(fonts: dict[str, bytes]) -> str:
    parts = [HEADER]
    for name in FONTS:
        data = fonts[name]
        info = font_info(data)
        glyphs, atlas, _pop = atlas_size(data)
        doc = FONT_DOC.format(
            name=name.upper(),
            glyphs=glyphs,
            w=s8(info["max_char_width"]),
            h=s8(info["max_char_height"]),
            ascent=s8(info["ascent_a"]),
            descent=s8(info["descent_g"]),
            rle=len(data),
            atlas=atlas,
            ratio=(atlas / len(data)) if len(data) else 0.0,
        )
        parts.append(doc.rstrip("\n") + "\n" + rust_array(name.upper(), data))
        parts.append("")
    return "\n".join(parts).rstrip() + "\n"


# --------------------------------------------------------------------------


def find_u8g2_fonts_c(explicit: str | None) -> Path:
    """Locate the U8g2 `u8g2_fonts.c` that the firmware links against."""
    if explicit:
        path = Path(explicit)
        if not path.is_file():
            sys.exit(f"extract_fonts: no such file: {path}")
        return path

    repo = Path(__file__).resolve().parents[3]
    candidates = sorted((repo / ".pio" / "libdeps").glob("*/U8g2/src/clib/u8g2_fonts.c"))
    if not candidates:
        sys.exit(
            "extract_fonts: could not find .pio/libdeps/*/U8g2/src/clib/u8g2_fonts.c.\n"
            "              Run `pio run -e esp32_usb` (or `pio pkg install -e esp32_usb`)\n"
            "              so PlatformIO fetches the U8g2 the firmware links, or pass\n"
            "              the path explicitly."
        )
    return candidates[0]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("mode", choices=("extract", "check", "report"))
    ap.add_argument("--u8g2", help="path to u8g2_fonts.c (default: .pio/libdeps/*/U8g2)")
    ap.add_argument(
        "--out",
        default=str(Path(__file__).resolve().parents[1] / "src" / "font" / "data.rs"),
    )
    args = ap.parse_args()

    src = find_u8g2_fonts_c(args.u8g2)
    fonts = parse_fonts_c(src.read_text(encoding="utf-8", errors="surrogateescape"))

    missing = [n for n in FONTS if n not in fonts]
    if missing:
        sys.exit(f"extract_fonts: {src} is missing: {', '.join(missing)}")

    if args.mode == "report":
        total_rle = 0
        total_atlas = 0
        print(f"U8g2 source: {src}")
        print()
        print(f"{'font':<28} {'RLE bytes':>10} {'ImageRaw':>10} {'ratio':>7} {'bbox':>9}")
        print("-" * 68)
        for name in FONTS:
            data = fonts[name]
            info = font_info(data)
            _g, atlas, _p = atlas_size(data)
            total_rle += len(data)
            total_atlas += atlas
            print(
                f"{name:<28} {len(data):>10} {atlas:>10} {atlas / len(data):>6.1f}x "
                f"{info['max_char_width']:>4}x{info['max_char_height']:<4}"
            )
        print("-" * 68)
        print(f"{'TOTAL':<28} {total_rle:>10} {total_atlas:>10} {total_atlas / total_rle:>6.1f}x")
        return 0

    generated = emit({n: fonts[n] for n in FONTS})
    out = Path(args.out)

    if args.mode == "extract":
        out.write_text(generated, encoding="utf-8")
        print(f"wrote {out} ({len(generated)} bytes of Rust)")
        return 0

    # check
    if not out.is_file():
        print(f"extract_fonts: check FAILED: {out} does not exist", file=sys.stderr)
        return 1
    if out.read_text(encoding="utf-8") != generated:
        print(
            f"extract_fonts: check FAILED: {out} is not what {src} produces.\n"
            f"              Run: crates/cc-display/tools/run.sh",
            file=sys.stderr,
        )
        return 1
    print(f"extract_fonts: check OK ({out} matches {src})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
