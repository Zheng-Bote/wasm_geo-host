import struct
import zlib
import os

XMP_PACKET = b"""<?xpacket begin="\xef\xbb\xbf" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/">
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">WASM Geo Sample</rdf:li></rdf:Alt></dc:title>
   <dc:creator><rdf:Seq><rdf:li>WASM Geo Platform</rdf:li></rdf:Seq></dc:creator>
   <dc:description><rdf:Alt><rdf:li xml:lang="x-default">Media item containing EXIF, IPTC and XMP metadata</rdf:li></rdf:Alt></dc:description>
   <photoshop:City>Berlin</photoshop:City>
   <photoshop:Country>Germany</photoshop:Country>
   <xmp:CreateDate>2026-11-25T10:00:00Z</xmp:CreateDate>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"""

# EXIF tag types
ASCII, SHORT, LONG, RATIONAL, BYTE = 2, 3, 4, 5, 1
TYPE_SIZE = {BYTE: 1, ASCII: 1, SHORT: 2, LONG: 4, RATIONAL: 8}

GPS_LAT, GPS_LON, GPS_ALT = 52.520008, 13.404954, 34.0   # Berlin, Germany


def _ascii(text):
    return (ASCII, len(text) + 1, text.encode() + b"\x00")


def _rationals(*pairs):
    return (RATIONAL, len(pairs), b"".join(struct.pack("<II", n, d) for n, d in pairs))


def _dms(value):
    deg = int(value)
    minutes_f = (value - deg) * 60
    minutes = int(minutes_f)
    seconds = round((minutes_f - minutes) * 60 * 10000)
    return _rationals((deg, 1), (minutes, 1), (seconds, 10000))


def _ifd(entries, start, next_ifd=0):
    """Serialises one IFD at offset `start`; returns (ifd_bytes, data_bytes, total_length).

    `entries` maps tag -> (type, count, value_bytes) or tag -> ("ptr", child) where `child`
    is a callable(offset) returning the serialised child IFD (bytes).
    """
    tags = sorted(entries)
    ifd_len = 2 + 12 * len(tags) + 4
    data = b""
    children = []
    body = struct.pack("<H", len(tags))
    for tag in tags:
        entry = entries[tag]
        if entry[0] == "ptr":
            children.append((tag, entry[1]))
            body += struct.pack("<HHI", tag, LONG, 1) + b"PTR!"  # patched below
            continue
        typ, count, value = entry
        if len(value) <= 4:
            body += struct.pack("<HHI", tag, typ, count) + value.ljust(4, b"\x00")
        else:
            offset = start + ifd_len + len(data)
            body += struct.pack("<HHII", tag, typ, count, offset)
            data += value + (b"\x00" if len(value) % 2 else b"")
    body += struct.pack("<I", next_ifd)
    blob = body + data
    for tag, child in children:
        child_offset = start + len(blob)
        blob = blob.replace(b"PTR!", struct.pack("<I", child_offset), 1)
        blob += child(child_offset)
    return blob


def build_exif_bytes():
    """Little-endian TIFF with IFD0, Exif IFD and GPS IFD (all values documented in README.md)."""
    exif_ifd = {
        0x829A: _rationals((1, 250)),              # ExposureTime 1/250 s
        0x829D: _rationals((28, 10)),              # FNumber f/2.8
        0x8827: (SHORT, 1, struct.pack("<H", 100)),  # ISOSpeedRatings 100
        0x9003: _ascii("2026:11:25 10:00:00"),     # DateTimeOriginal
        0x920A: _rationals((35, 1)),               # FocalLength 35 mm
        0xA434: _ascii("FE 24-70mm F2.8 GM"),      # LensModel
    }
    gps_ifd = {
        0x0000: (BYTE, 4, bytes([2, 3, 0, 0])),    # GPSVersionID 2.3.0.0
        0x0001: _ascii("N"),
        0x0002: _dms(GPS_LAT),
        0x0003: _ascii("E"),
        0x0004: _dms(GPS_LON),
        0x0005: (BYTE, 1, b"\x00"),               # above sea level
        0x0006: _rationals((int(GPS_ALT), 1)),
    }
    ifd0 = {
        0x010F: _ascii("Sony"),
        0x0110: _ascii("ILCE-7RM4"),
        0x0132: _ascii("2026:11:25 10:00:00"),
        0x8769: ("ptr", lambda off: _ifd(exif_ifd, off)),
        0x8825: ("ptr", lambda off: _ifd(gps_ifd, off)),
    }
    return b"II\x2a\x00" + struct.pack("<I", 8) + _ifd(ifd0, 8)


class _BitWriter:
    def __init__(self):
        self.out, self.acc, self.n = bytearray(), 0, 0

    def write(self, code, length):
        for i in range(length - 1, -1, -1):
            self.acc = (self.acc << 1) | ((code >> i) & 1)
            self.n += 1
            if self.n == 8:
                self.out.append(self.acc)
                if self.acc == 0xFF:
                    self.out.append(0x00)  # byte stuffing
                self.acc, self.n = 0, 0

    def flush(self):
        if self.n:
            self.write((1 << (8 - self.n)) - 1, 8 - self.n)
        return bytes(self.out)


def build_jpeg_image(width=64, height=48):
    """Minimal baseline JPEG (grayscale, DC-only) with a horizontal gradient of 8x8 blocks.

    Every block is flat; its DC difference is coded with the standard luminance DC Huffman
    table, and the AC part is a single EOB (one-symbol AC table).
    """
    dc_bits = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0]
    dc_vals = list(range(12))
    # canonical Huffman codes for the DC table
    codes, code, k = {}, 0, 0
    for length, count in enumerate(dc_bits, start=1):
        for _ in range(count):
            codes[dc_vals[k]] = (code, length)
            code += 1
            k += 1
        code <<= 1

    quant = 8  # flat quantisation table
    blocks_x, blocks_y = width // 8, height // 8
    bw = _BitWriter()
    prev_dc = 0
    for by in range(blocks_y):
        for bx in range(blocks_x):
            level = 32 + bx * 192 // max(blocks_x - 1, 1)          # 32..224
            dc = round((level - 128) * 8 / quant)                    # DCT DC coefficient / q
            diff = dc - prev_dc
            prev_dc = dc
            cat = 0 if diff == 0 else abs(diff).bit_length()
            c, l = codes[cat]
            bw.write(c, l)
            if cat:
                bw.write(diff if diff > 0 else diff + (1 << cat) - 1, cat)
            bw.write(0, 1)  # AC: EOB (only symbol of the AC table, code "0")
    scan = bw.flush()

    def segment(marker, payload):
        return struct.pack(">HH", marker, len(payload) + 2) + payload

    dqt = segment(0xFFDB, b"\x00" + bytes([quant] * 64))
    sof0 = segment(0xFFC0, struct.pack(">BHHB", 8, height, width, 1) + b"\x01\x11\x00")
    dht_dc = segment(0xFFC4, b"\x00" + bytes(dc_bits) + bytes(dc_vals))
    dht_ac = segment(0xFFC4, b"\x10" + bytes([1] + [0] * 15) + b"\x00")
    sos = segment(0xFFDA, b"\x01\x01\x00\x00\x3f\x00")
    return dqt + sof0 + dht_dc + dht_ac + sos + scan + b"\xFF\xD9"

def build_iptc_bytes():
    # Photoshop 3.0 8BIM header containing IPTC-NAA record
    # IPTC record 2:05 (ObjectName), 2:80 (Byline), 2:101 (Country), 2:120 (Caption)
    records = []
    def add_tag(dataset, val_bytes):
        # 0x1C, record_set 2, dataset, len
        return struct.pack('>BBBH', 0x1C, 2, dataset, len(val_bytes)) + val_bytes
    
    iptc_data = (
        add_tag(5, b"WASM Geo Sample") +
        add_tag(80, b"WASM Geo Platform") +
        add_tag(101, b"Germany") +
        add_tag(120, b"Testing EXIF, IPTC and XMP metadata extraction")
    )
    
    # Wrap in Photoshop 8BIM block
    # '8BIM', resource ID 0x0404 (IPTC-NAA), Pascal string name (0), length of data (4 bytes)
    header = b'Photoshop 3.0\x008BIM\x04\x04\x00\x00' + struct.pack('>I', len(iptc_data))
    return header + iptc_data

def generate_jpeg(out_path):
    exif_payload = b"Exif\x00\x00" + build_exif_bytes()
    app1_exif = struct.pack('>HH', 0xFFE1, len(exif_payload) + 2) + exif_payload
    
    xmp_payload = b"http://ns.adobe.com/xap/1.0/\x00" + XMP_PACKET
    app1_xmp = struct.pack('>HH', 0xFFE1, len(xmp_payload) + 2) + xmp_payload
    
    iptc_payload = build_iptc_bytes()
    app13_iptc = struct.pack('>HH', 0xFFED, len(iptc_payload) + 2) + iptc_payload
    
    # Decodable 64x48 grayscale gradient
    jpeg_bytes = b'\xFF\xD8' + app1_exif + app1_xmp + app13_iptc + build_jpeg_image()
    with open(out_path, 'wb') as f:
        f.write(jpeg_bytes)
    print(f"Generated JPEG: {out_path} ({len(jpeg_bytes)} bytes)")

def make_png_chunk(chunk_type, data):
    length = len(data)
    crc = zlib.crc32(chunk_type + data) & 0xffffffff
    return struct.pack('>I', length) + chunk_type + data + struct.pack('>I', crc)

def generate_png(out_path):
    sig = b'\x89PNG\r\n\x1a\n'
    # IHDR: 1x1, 8-bit RGB
    ihdr = make_png_chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 2, 0, 0, 0))
    # eXIf chunk with TIFF EXIF data
    exif_chunk = make_png_chunk(b'eXIf', build_exif_bytes())
    # iTXt chunk with XMP
    itxt_xmp = make_png_chunk(b'iTXt', b'XML:com.adobe.xmp\x00\x00\x00\x00\x00' + XMP_PACKET)
    # tEXt chunk with IPTC information
    text_iptc = make_png_chunk(b'tEXt', b'IPTC:Caption\x00Testing EXIF, IPTC and XMP in PNG')
    
    # IDAT: compressed scanline (filter 0 + RGB 0,0,0)
    raw_data = b'\x00\x00\x00\x00'
    idat = make_png_chunk(b'IDAT', zlib.compress(raw_data))
    iend = make_png_chunk(b'IEND', b'')
    
    png_bytes = sig + ihdr + exif_chunk + itxt_xmp + text_iptc + idat + iend
    with open(out_path, 'wb') as f:
        f.write(png_bytes)
    print(f"Generated PNG: {out_path} ({len(png_bytes)} bytes)")

def make_box(box_type, payload):
    return struct.pack('>I', len(payload) + 8) + box_type + payload

def generate_heic(out_path):
    # Standard ISOBMFF HEIC container
    # ftyp box
    ftyp = make_box(b'ftyp', b'mif1\x00\x00\x00\x00heicmif1')
    
    # idat box with EXIF and XMP payloads
    exif_data = b'\x00\x00\x00\x06Exif\x00\x00' + build_exif_bytes()
    idat = make_box(b'idat', exif_data + XMP_PACKET)
    
    # meta box with handler 'pict'
    hdlr = make_box(b'hdlr', b'\x00\x00\x00\x00\x00\x00\x00\x00pict\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00PictureHandler\x00')
    meta = struct.pack('>I', len(hdlr) + 12) + b'meta\x00\x00\x00\x00' + hdlr
    
    # mdat dummy image data
    mdat = make_box(b'mdat', b'\x00\x00\x00\x01\x67\x42\x00\x0a')
    
    heic_bytes = ftyp + meta + idat + mdat
    with open(out_path, 'wb') as f:
        f.write(heic_bytes)
    print(f"Generated HEIC: {out_path} ({len(heic_bytes)} bytes)")

if __name__ == '__main__':
    os.makedirs('Examples', exist_ok=True)
    generate_jpeg('Examples/sample.jpg')
    generate_png('Examples/sample.png')
    generate_heic('Examples/sample.heic')
