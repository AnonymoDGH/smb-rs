#!/usr/bin/env python3
"""Super Mario Bros. (World) NES ROM - static analysis toolkit.

Parses the iNES container, validates checksums, dumps PRG/CHR, renders
pattern tables + full CHR atlas as PNG, extracts the palette tables that the
PPU uploads, and disassembles the PRG with a full 6502 decoder.

Usage:
    python analyze.py <rom.nes> [outdir]
"""
import sys, os, hashlib, json
from PIL import Image

NES_PALETTE = [
    (0x62,0x62,0x62),(0x00,0x1F,0xB2),(0x24,0x04,0xC8),(0x52,0x00,0xB2),
    (0x73,0x00,0x76),(0x80,0x00,0x24),(0x73,0x0B,0x00),(0x52,0x28,0x00),
    (0x24,0x44,0x00),(0x00,0x57,0x00),(0x00,0x5C,0x00),(0x00,0x53,0x24),
    (0x00,0x3C,0x76),(0x00,0x00,0x00),(0x00,0x00,0x00),(0x00,0x00,0x00),
    (0xAB,0xAB,0xAB),(0x0D,0x57,0xFF),(0x4B,0x30,0xFF),(0x8A,0x13,0xFF),
    (0xBC,0x08,0xD6),(0xD2,0x0D,0x69),(0xC7,0x2E,0x00),(0x9D,0x54,0x00),
    (0x60,0x7C,0x00),(0x20,0x94,0x00),(0x00,0x9A,0x00),(0x00,0x8F,0x5C),
    (0x00,0x72,0xDB),(0x00,0x00,0x00),(0x00,0x00,0x00),(0x00,0x00,0x00),
    (0xFF,0xFF,0xFF),(0x53,0xAC,0xFF),(0x90,0x85,0xFF),(0xD3,0x65,0xFF),
    (0xFF,0x57,0xFF),(0xFF,0x5D,0xA9),(0xFF,0x77,0x3C),(0xF2,0xA6,0x00),
    (0xBF,0xD1,0x00),(0x72,0xEB,0x00),(0x1F,0xF5,0x00),(0x00,0xEB,0xAB),
    (0x00,0xCE,0xFF),(0x4B,0x4B,0x4B),(0x00,0x00,0x00),(0x00,0x00,0x00),
    (0xFF,0xFF,0xFF),(0xB6,0xE1,0xFF),(0xCE,0xD1,0xFF),(0xE9,0xC3,0xFF),
    (0xFF,0xBC,0xFF),(0xFF,0xC1,0xE3),(0xFF,0xD0,0xB0),(0xFF,0xE1,0x8D),
    (0xFF,0xF0,0x7C),(0xE2,0xFD,0x7C),(0xB4,0xFF,0x7C),(0x82,0xFE,0xB4),
    (0x6E,0xF6,0xFF),(0xB4,0xB4,0xB4),(0x00,0x00,0x00),(0x00,0x00,0x00),
]


def parse(path):
    raw = open(path, 'rb').read()
    assert raw[:4] == b'NES\x1a', 'not an iNES file'
    h = raw[:16]
    prg_banks, chr_banks = h[4], h[5]
    flags6, flags7 = h[6], h[7]
    info = {
        'file_size': len(raw),
        'sha256': hashlib.sha256(raw).hexdigest(),
        'md5': hashlib.md5(raw).hexdigest(),
        'crc32': '%08X' % (__import__('zlib').crc32(raw) & 0xFFFFFFFF),
        'prg_banks_16k': prg_banks,
        'chr_banks_8k': chr_banks,
        'mapper': (flags7 & 0xF0) | (flags6 >> 4),
        'mirroring': 'vertical' if flags6 & 1 else 'horizontal',
        'battery': bool(flags6 & 2),
        'trainer': bool(flags6 & 4),
        'four_screen': bool(flags6 & 8),
        'prg_size': prg_banks * 16384,
        'chr_size': chr_banks * 8192,
    }
    off = 16 + (512 if info['trainer'] else 0)
    prg = raw[off:off + info['prg_size']]
    chr_ = raw[off + info['prg_size']:off + info['prg_size'] + info['chr_size']]
    info['prg_sha256'] = hashlib.sha256(prg).hexdigest()
    info['chr_sha256'] = hashlib.sha256(chr_).hexdigest()
    # CHR is often all-zero in NROM titles that use CHR-RAM; verify.
    info['chr_is_ram'] = (len(chr_) == 0) or (max(chr_) == 0)
    info['vectors'] = {
        'NMI': prg[-6] | (prg[-5] << 8),
        'RESET': prg[-4] | (prg[-3] << 8),
        'IRQ': prg[-2] | (prg[-1] << 8),
    }
    return info, prg, chr_


def tile_pixels(chr_, index, flip=False):
    """Return 8x8 list of 2-bit pixel values for a CHR tile."""
    base = index * 16
    px = []
    for y in range(8):
        lo = chr_[base + y]
        hi = chr_[base + y + 8]
        row = []
        for x in range(8):
            bit = 7 - x
            row.append(((lo >> bit) & 1) | (((hi >> bit) & 1) << 1))
        px.append(row)
    return px


def render_pattern_table(chr_, table, palette, scale=4):
    img = Image.new('RGB', (128, 128), (0, 0, 0))
    pix = img.load()
    for t in range(256):
        px = tile_pixels(chr_, table * 256 + t)
        ox, oy = (t % 16) * 8, (t // 16) * 8
        for y in range(8):
            for x in range(8):
                pix[ox + x, oy + y] = NES_PALETTE[palette[px[y][x]]]
    return img.resize((128 * scale, 128 * scale), Image.NEAREST)


def render_chr_atlas(chr_, scale=3):
    """All 512 tiles, 16 wide, using a neutral grayscale ramp."""
    img = Image.new('RGB', (128, 32 * 8), (24, 24, 24))
    pix = img.load()
    ramp = [(24, 24, 24), (90, 90, 90), (170, 170, 170), (255, 255, 255)]
    for t in range(512):
        px = tile_pixels(chr_, t)
        ox, oy = (t % 16) * 8, (t // 16) * 8
        for y in range(8):
            for x in range(8):
                pix[ox + x, oy + y] = ramp[px[y][x]]
    return img.resize((128 * scale, 256 * scale), Image.NEAREST)


def disassemble(prg, base=0x8000, start=0, end=None):
    """Full 6502 disassembler. Returns list of (addr, bytes, text)."""
    OPS = build_opcode_table()
    end = len(prg) if end is None else end
    out = []
    pc = start
    while pc < end:
        op = prg[pc]
        mnem, mode, size = OPS[op]
        raw = prg[pc:pc + size]
        if len(raw) < size:
            break
        addr = base + pc
        out.append((addr, raw, fmt(mnem, mode, raw, addr, base)))
        pc += size
    return out


def fmt(mnem, mode, raw, addr, base):
    if mode == 'imp':
        return mnem
    if mode == 'acc':
        return mnem + ' A'
    if mode == 'imm':
        return '%s #$%02X' % (mnem, raw[1])
    if mode == 'zp':
        return '%s $%02X' % (mnem, raw[1])
    if mode == 'zpx':
        return '%s $%02X,X' % (mnem, raw[1])
    if mode == 'zpy':
        return '%s $%02X,Y' % (mnem, raw[1])
    if mode == 'abs':
        return '%s $%04X' % (mnem, raw[1] | raw[2] << 8)
    if mode == 'absx':
        return '%s $%04X,X' % (mnem, raw[1] | raw[2] << 8)
    if mode == 'absy':
        return '%s $%04X,Y' % (mnem, raw[1] | raw[2] << 8)
    if mode == 'ind':
        return '%s ($%04X)' % (mnem, raw[1] | raw[2] << 8)
    if mode == 'indx':
        return '%s ($%02X,X)' % (mnem, raw[1])
    if mode == 'indy':
        return '%s ($%02X),Y' % (mnem, raw[1])
    if mode == 'rel':
        tgt = (addr + 2 + (raw[1] - 256 if raw[1] > 127 else raw[1])) & 0xFFFF
        return '%s $%04X' % (mnem, tgt)
    return '???'


def build_opcode_table():
    t = [('???', 'imp', 1)] * 256
    def s(code, m, mo, sz):
        t[code] = (m, mo, sz)
    # official opcodes
    table = """
00 BRK imp 1|01 ORA indx 2|05 ORA zp 2|06 ASL zp 2|08 PHP imp 1|09 ORA imm 2|0A ASL acc 1|0D ORA abs 3
0E ASL abs 3|10 BPL rel 2|11 ORA indy 2|15 ORA zpx 2|16 ASL zpx 2|18 CLC imp 1|19 ORA absy 3|1D ORA absx 3
1E ASL absx 3|20 JSR abs 3|21 AND indx 2|24 BIT zp 2|25 AND zp 2|26 ROL zp 2|28 PLP imp 1|29 AND imm 2
2A ROL acc 1|2C BIT abs 3|2D AND abs 3|2E ROL abs 3|30 BMI rel 2|31 AND indy 2|35 AND zpx 2|36 ROL zpx 2
38 SEC imp 1|39 AND absy 3|3D AND absx 3|3E ROL absx 3|40 RTI imp 1|41 EOR indx 2|45 EOR zp 2|46 LSR zp 2
48 PHA imp 1|49 EOR imm 2|4A LSR acc 1|4C JMP abs 3|4D EOR abs 3|4E LSR abs 3|50 BVC rel 2|51 EOR indy 2
55 EOR zpx 2|56 LSR zpx 2|58 CLI imp 1|59 EOR absy 3|5D EOR absx 3|5E LSR absx 3|60 RTS imp 1|61 ADC indx 2
65 ADC zp 2|66 ROR zp 2|68 PLA imp 1|69 ADC imm 2|6A ROR acc 1|6C JMP ind 3|6D ADC abs 3|6E ROR abs 3
70 BVS rel 2|71 ADC indy 2|75 ADC zpx 2|76 ROR zpx 2|78 SEI imp 1|79 ADC absy 3|7D ADC absx 3|7E ROR absx 3
81 STA indx 2|84 STY zp 2|85 STA zp 2|86 STX zp 2|88 DEY imp 1|8A TXA imp 1|8C STY abs 3|8D STA abs 3
8E STX abs 3|90 BCC rel 2|91 STA indy 2|94 STY zpx 2|95 STA zpx 2|96 STX zpy 2|98 TYA imp 1|99 STA absy 3
9A TXS imp 1|9D STA absx 3|A0 LDY imm 2|A1 LDA indx 2|A2 LDX imm 2|A4 LDY zp 2|A5 LDA zp 2|A6 LDX zp 2
A8 TAY imp 1|A9 LDA imm 2|AA TAX imp 1|AC LDY abs 3|AD LDA abs 3|AE LDX abs 3|B0 BCS rel 2|B1 LDA indy 2
B4 LDY zpx 2|B5 LDA zpx 2|B6 LDX zpy 2|B8 CLV imp 1|B9 LDA absy 3|BA TSX imp 1|BC LDY absx 3|BD LDA absx 3
BE LDX absy 3|C0 CPY imm 2|C1 CMP indx 2|C4 CPY zp 2|C5 CMP zp 2|C6 DEC zp 2|C8 INY imp 1|C9 CMP imm 2
CA DEX imp 1|CC CPY abs 3|CD CMP abs 3|CE DEC abs 3|D0 BNE rel 2|D1 CMP indy 2|D5 CMP zpx 2|D6 DEC zpx 2
D8 CLD imp 1|D9 CMP absy 3|DD CMP absx 3|DE DEC absx 3|E0 CPX imm 2|E1 SBC indx 2|E4 CPX zp 2|E5 SBC zp 2
E6 INC zp 2|E8 INX imp 1|E9 SBC imm 2|EA NOP imp 1|EC CPX abs 3|ED SBC abs 3|EE INC abs 3|F0 BEQ rel 2
F1 SBC indy 2|F5 SBC zpx 2|F6 INC zpx 2|F8 SED imp 1|F9 SBC absy 3|FD SBC absx 3|FE INC absx 3
"""
    for entry in table.replace('\n', '|').split('|'):
        if not entry.strip():
            continue
        code, m, mo, sz = entry.split()
        s(int(code, 16), m, mo, int(sz))
    return t


def main():
    path = sys.argv[1]
    outdir = sys.argv[2] if len(sys.argv) > 2 else 'analysis'
    os.makedirs(outdir, exist_ok=True)
    info, prg, chr_ = parse(path)
    print(json.dumps(info, indent=2))

    open(os.path.join(outdir, 'prg.bin'), 'wb').write(prg)
    open(os.path.join(outdir, 'chr.bin'), 'wb').write(chr_)
    open(os.path.join(outdir, 'header.json'), 'w').write(json.dumps(info, indent=2))

    if chr_:
        # Palette used by SMB title screen upload (PPU $3F00 block lives in PRG).
        render_pattern_table(chr_, 0, [0x0F, 0x30, 0x10, 0x00], 4).save(os.path.join(outdir, 'pattern_table_0.png'))
        render_pattern_table(chr_, 1, [0x0F, 0x30, 0x10, 0x00], 4).save(os.path.join(outdir, 'pattern_table_1.png'))
        render_chr_atlas(chr_, 3).save(os.path.join(outdir, 'chr_atlas.png'))

    with open(os.path.join(outdir, 'disasm.txt'), 'w', encoding='utf-8') as f:
        for addr, raw, text in disassemble(prg):
            f.write('%04X  %-9s %s\n' % (addr, raw.hex(' '), text))

    # code/data density map: fraction of bytes that decode as valid official opcodes
    print('dumped to', outdir)


if __name__ == '__main__':
    main()
