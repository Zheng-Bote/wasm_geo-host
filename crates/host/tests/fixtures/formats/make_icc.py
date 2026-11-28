"""Write a minimal ICC v2 RGB display profile (sRGB primaries, D50-adapted, gamma 2.2).

Self-authored test data (CC0). Usage: python3 -I make_icc.py <out.icc>
"""
import struct
import sys


def s15f16(v):
    return struct.pack(">i", int(round(v * 65536)))


def xyz(x, y, z):
    return b"XYZ " + b"\0" * 4 + s15f16(x) + s15f16(y) + s15f16(z)


def curv(gamma):
    return b"curv" + b"\0" * 4 + struct.pack(">I", 1) + struct.pack(">H", int(gamma * 256)) + b"\0\0"


def desc(text):
    t = text.encode("ascii") + b"\0"
    body = b"desc" + b"\0" * 4 + struct.pack(">I", len(t)) + t
    body += struct.pack(">II", 0, 0)  # unicode language code, count
    body += struct.pack(">HB", 0, 0) + b"\0" * 67  # scriptcode
    return body


def text(t):
    return b"text" + b"\0" * 4 + t.encode("ascii") + b"\0"


tags = [
    (b"desc", desc("flagme test sRGB-like gamma 2.2")),
    (b"cprt", text("CC0 test profile, wasm_geo-host")),
    (b"wtpt", xyz(0.9642, 1.0, 0.8249)),
    (b"rXYZ", xyz(0.4361, 0.2225, 0.0139)),
    (b"gXYZ", xyz(0.3851, 0.7169, 0.0971)),
    (b"bXYZ", xyz(0.1431, 0.0606, 0.7141)),
    (b"rTRC", curv(2.2)),
    (b"gTRC", curv(2.2)),
    (b"bTRC", curv(2.2)),
]

offset = 128 + 4 + 12 * len(tags)
table = struct.pack(">I", len(tags))
data = b""
for sig, body in tags:
    while (offset + len(data)) % 4:
        data += b"\0"
    table += sig + struct.pack(">II", offset + len(data), len(body))
    data += body
while len(data) % 4:
    data += b"\0"

size = 128 + len(table) + len(data)
header = struct.pack(">I", size) + b"none" + bytes([2, 0x10, 0, 0]) + b"mntr" + b"RGB " + b"XYZ "
header += struct.pack(">HHHHHH", 2026, 11, 28, 0, 0, 0) + b"acsp" + b"APPL" + struct.pack(">I", 0)
header += b"\0" * 4 + b"\0" * 4 + b"\0" * 8 + struct.pack(">I", 0)
header += s15f16(0.9642) + s15f16(1.0) + s15f16(0.8249) + b"none"
header += b"\0" * (128 - len(header))
assert len(header) == 128
open(sys.argv[1], "wb").write(header + table + data)
