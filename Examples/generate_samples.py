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

def build_exif_bytes():
    # Little-endian TIFF header: 'II\x2a\x00', IFD offset = 8
    # IFD0 with Make, Model, DateTimeOriginal, and ExifIFD pointer
    tiff_header = b'II\x2a\x00\x08\x00\x00\x00'
    
    # Values offsets
    # Header: 8 bytes
    # IFD0: 2 bytes (count=4) + 4 * 12 bytes = 50 bytes + 4 bytes next_ifd = 54 bytes -> Offset = 62
    make_bytes = b"Sony\x00"
    model_bytes = b"ILCE-7RM4\x00"
    date_bytes = b"2026:11:25 10:00:00\x00"
    
    offset_make = 8 + 2 + (4 * 12) + 4
    offset_model = offset_make + len(make_bytes)
    offset_date = offset_model + len(model_bytes)
    
    entries = []
    # Tag 0x010F: Make (ASCII)
    entries.append(struct.pack('<HHII', 0x010F, 2, len(make_bytes), offset_make))
    # Tag 0x0110: Model (ASCII)
    entries.append(struct.pack('<HHII', 0x0110, 2, len(model_bytes), offset_model))
    # Tag 0x0132: DateTime (ASCII)
    entries.append(struct.pack('<HHII', 0x0132, 2, len(date_bytes), offset_date))
    # Tag 0x8769: ExifOffset
    entries.append(struct.pack('<HHII', 0x8769, 4, 1, 0)) # inline or dummy
    
    ifd0 = struct.pack('<H', len(entries)) + b''.join(entries) + struct.pack('<I', 0)
    data_payload = make_bytes + model_bytes + date_bytes
    
    return tiff_header + ifd0 + data_payload

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
    
    # Minimal 1x1 JPEG stream
    soi = b'\xFF\xD8'
    dqt = b'\xFF\xDB\x00\x43\x00' + (b'\x10' * 64)
    sof0 = b'\xFF\xC0\x00\x0B\x08\x00\x01\x00\x01\x01\x01\x11\x00'
    dht = b'\xFF\xC4\x00\x1F\x00\x00\x01\x05\x01\x01\x01\x01\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B'
    sos = b'\xFF\xDA\x00\x08\x01\x01\x00\x00\x3F\x00\xD2\xCF\x20'
    eoi = b'\xFF\xD9'
    
    jpeg_bytes = soi + app1_exif + app1_xmp + app13_iptc + dqt + sof0 + dht + sos + eoi
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
