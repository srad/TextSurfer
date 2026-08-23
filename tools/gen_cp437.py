#!/usr/bin/env python3
"""Generate src/vga/font/table.rs — CP437 8x16 bitmaps and the CP437 <-> Unicode map.

Source: pcface's Modern DOS 8x16 font list, which is available under the MIT
License or CC0 1.0 (the underlying Modern DOS font is itself CC0). The Oldschool
PC bitmaps in the same repository are GPL/CC-BY-SA and are deliberately NOT used.

    https://github.com/susam/pcface  ->  out/moderndos-8x16/fontlist.js

Verification: pcface also publishes glyph.txt, its own '*'/space rendering of every
glyph. This script parses fontlist.js, re-renders it to that exact format, and
requires a byte-identical match before writing anything. That checks the parse
against an independent upstream artifact rather than against itself. The emitted
table carries a SHA-256 that `vga::font::tests` pins, so silent drift fails a gate.

Usage (from the repository root):

    python tools/gen_cp437.py
"""

import hashlib
import pathlib
import re
import sys
import urllib.request

# Pinned tree so regeneration is reproducible.
COMMIT = "8629ba46b73f58ac88cacbe8e77c6e9975dd221c"
BASE = f"https://raw.githubusercontent.com/susam/pcface/{COMMIT}/out/moderndos-8x16"
OUT = pathlib.Path(__file__).resolve().parent.parent / "src" / "vga" / "font" / "table.rs"

ROW_RE = re.compile(
    r"^\s*\[((?:0x[0-9a-fA-F]{2},\s*){15}0x[0-9a-fA-F]{2})\],\s*//\s*\[(.*)\]\s*\((\d+)\)\s*$"
)

# Bytes 0x01-0x1F and 0x7F are C0 controls in the codec, but an IBM PC in text mode
# draws graphics there. Those glyphs are real and reachable (arrows and geometric
# shapes turn up in page content), so map them the way the hardware did. Index 0x00
# is deliberately absent: the codec already yields U+0000 for it, which keeps it
# distinct from CP437 0x20 while drawing the blank glyph the hardware drew.
GRAPHIC_CONTROLS = {
    0x01: "☺", 0x02: "☻", 0x03: "♥",
    0x04: "♦", 0x05: "♣", 0x06: "♠", 0x07: "•",
    0x08: "◘", 0x09: "○", 0x0A: "◙", 0x0B: "♂",
    0x0C: "♀", 0x0D: "♪", 0x0E: "♫", 0x0F: "☼",
    0x10: "►", 0x11: "◄", 0x12: "↕", 0x13: "‼",
    0x14: "¶", 0x15: "§", 0x16: "▬", 0x17: "↨",
    0x18: "↑", 0x19: "↓", 0x1A: "→", 0x1B: "←",
    0x1C: "∟", 0x1D: "↔", 0x1E: "▲", 0x1F: "▼",
    0x7F: "⌂",
}


def fetch(name):
    with urllib.request.urlopen(f"{BASE}/{name}") as response:
        return response.read().decode("utf-8")


def parse_fontlist(text):
    glyphs, labels = [], []
    for line in text.splitlines():
        match = ROW_RE.match(line)
        if not match:
            continue
        rows = [int(token, 16) for token in match.group(1).split(",")]
        if len(rows) != 16:
            sys.exit(f"FAIL: glyph {match.group(3)} has {len(rows)} rows, expected 16")
        if int(match.group(3)) != len(glyphs):
            sys.exit(f"FAIL: glyph index {match.group(3)} out of order")
        glyphs.append(rows)
        labels.append(match.group(2))
    return glyphs, labels


def render(glyphs, labels):
    """Re-render to pcface's glyph.txt format, exactly."""
    out = []
    for index, (rows, label) in enumerate(zip(glyphs, labels)):
        out.append(f"----- [{label}] ({index}) -----")
        for row in rows:
            out.append("".join("*" if row & (1 << (7 - bit)) else " " for bit in range(8)))
    out.append("----- END -----")
    return "\n".join(out) + "\n"


def unicode_table():
    """CP437 index -> Unicode scalar, using Python's codec plus the graphic controls."""
    return [
        GRAPHIC_CONTROLS[index]
        if index in GRAPHIC_CONTROLS
        else bytes([index]).decode("cp437")
        for index in range(256)
    ]


def escape(char):
    """Rust char literal, escaping what Rust requires plus anything non-printable."""
    if char in ("\\", "'"):
        return f"'\\{char}'"
    if ord(char) < 0x20 or ord(char) == 0x7F:
        return f"'\\u{{{ord(char):x}}}'"
    # Invisible-but-printable scalars (NBSP) would read as a plain space in source.
    if char.isspace() and char != " ":
        return f"'\\u{{{ord(char):x}}}'"
    return f"'{char}'"


def emit(glyphs, digest):
    chars = unicode_table()

    # A duplicate would make the reverse lookup ambiguous and silently pick one arm.
    seen = {}
    for index, char in enumerate(chars):
        if char in seen:
            sys.exit(f"FAIL: U+{ord(char):04X} maps to both {seen[char]} and {index}")
        seen[char] = index

    lines = [
        "//! CP437 8x16 glyph bitmaps and the CP437 <-> Unicode mapping.",
        "//! Generated — do not edit by hand.",
        "//!",
        "//! Regenerate with `python tools/gen_cp437.py`, which re-derives the bitmaps from",
        "//! pcface's Modern DOS 8x16 font (MIT or CC0), verifies the parse against pcface's",
        "//! own `glyph.txt` rendering, and derives the character mapping from Python's",
        "//! `cp437` codec plus the IBM graphic interpretations of the C0 range.",
        "//!",
        "//! Bitmap layout: one byte per pixel row, most significant bit leftmost.",
        "",
        "/// SHA-256 of [`CP437_8X16`]'s bytes in glyph-then-row order.",
        "///",
        "/// Pinned by `super::tests` so a regeneration that changes the glyphs fails a gate",
        "/// instead of silently altering every rendered frame.",
        f'pub const CP437_8X16_SHA256: &str =\n    "{digest}";',
        "",
        "/// The 256 CP437 glyphs, 16 rows of 8 pixels each.",
        # One glyph per line is what makes this table reviewable; rustfmt would reflow
        # it into an unreadable ribbon, and `cargo fmt --check` is a gate.
        "#[rustfmt::skip]",
        "pub const CP437_8X16: [[u8; 16]; 256] = [",
    ]
    for index, rows in enumerate(glyphs):
        body = ", ".join(f"0x{row:02x}" for row in rows)
        lines.append(f"    [{body}], // {index}")
    lines.extend(
        [
            "];",
            "",
            "/// CP437 index to the Unicode scalar it draws.",
            "#[rustfmt::skip]",
            "pub const CP437_TO_UNICODE: [char; 256] = [",
        ]
    )
    for start in range(0, 256, 8):
        row = ", ".join(escape(chars[index]) for index in range(start, start + 8))
        lines.append(f"    {row},")
    lines.extend(
        [
            "];",
            "",
            "/// Unicode scalar to CP437 index, sorted by scalar for binary search.",
            "///",
            "/// Excludes the ASCII range, which [`super::cp437_index`] answers directly.",
            "pub const UNICODE_TO_CP437: &[(char, u8)] = &[",
        ]
    )
    pairs = sorted(
        ((char, index) for index, char in enumerate(chars) if not 0x20 <= ord(char) <= 0x7E),
        key=lambda pair: ord(pair[0]),
    )
    for char, index in pairs:
        lines.append(f"    ({escape(char)}, 0x{index:02x}), // U+{ord(char):04X}")
    lines.append("];")
    return "\n".join(lines) + "\n"


def main():
    glyphs, labels = parse_fontlist(fetch("fontlist.js"))
    if len(glyphs) != 256:
        sys.exit(f"FAIL: parsed {len(glyphs)} glyphs, expected 256")

    expected = fetch("glyph.txt")
    actual = render(glyphs, labels)
    if actual != expected:
        for number, (a, b) in enumerate(zip(actual.splitlines(), expected.splitlines()), 1):
            if a != b:
                sys.exit(f"FAIL: glyph.txt mismatch at line {number}: {a!r} != {b!r}")
        sys.exit("FAIL: glyph.txt length mismatch")

    flat = bytes(row for rows in glyphs for row in rows)
    digest = hashlib.sha256(flat).hexdigest()
    OUT.write_text(emit(glyphs, digest), encoding="utf-8", newline="\n")

    print("OK: 256 glyphs; re-render matches upstream glyph.txt byte-for-byte")
    print(f"OK: {len(flat)} bytes, sha256 {digest}")
    print(f"OK: wrote {OUT}")


if __name__ == "__main__":
    main()
