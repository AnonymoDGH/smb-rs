#!/usr/bin/env python3
"""Locate structural data inside the SMB PRG: text, palettes, level tables."""
import sys, os, json, re

prg = open('analysis/prg.bin','rb').read()
BASE = 0x8000

def a(i): return BASE + i

# --- 1. text strings encoded as CHR-table-1 tile indices -------------------
# font: 0x00='0' .. 0x09='9', 0x0A='A' .. 0x23='Z'
def decode_text(b):
    out = []
    for c in b:
        if 0 <= c <= 9: out.append(chr(ord('0')+c))
        elif 10 <= c <= 0x23: out.append(chr(ord('A')+c-10))
        elif c == 0x24: out.append('-')
        elif c == 0x28: out.append('!')
        elif c == 0x29: out.append('?')
        elif c == 0x2A: out.append('.')
        elif c == 0x2B: out.append(',')
        elif c == 0x2E: out.append('*')
        elif c == 0x2F: out.append(' ')
        elif c == 0xFE: out.append('/')
        else: out.append('.')
    return ''.join(out)

print('=== candidate text runs (>=4 font chars) ===')
runs = []
cur = []
start = 0
for i, c in enumerate(prg):
    if c <= 0x2F or c == 0xFE:
        if not cur: start = i
        cur.append(c)
    else:
        if len(cur) >= 4:
            runs.append((start, bytes(cur)))
        cur = []
for st, b in runs:
    txt = decode_text(b)
    if sum(ch.isalpha() for ch in txt) >= 3 or txt.strip('0'):
        print('%04X  %-40s  %s' % (a(st), txt, b.hex(' ')))

# --- 2. palette upload blocks ---------------------------------------------
print('\n=== PPU palette writes ===')
for i in range(len(prg)-4):
    if prg[i] == 0xA9 and prg[i+1] == 0x3F and prg[i+2] == 0x8D and prg[i+3] == 0x06 and prg[i+4] == 0x20:
        print('palette routine at %04X' % a(i))

# scan for palette-looking tables: first byte 0x0F or 0x3F-ish, 32 bytes
print('\n=== palette-like tables (16/32 bytes, NES-valid indices) ===')
for i in range(len(prg)-32):
    blk = prg[i:i+32]
    if blk[0] in (0x0F, 0x30, 0x22) and all(x < 0x40 for x in blk) and len(set(blk)) >= 12:
        print('%04X %s' % (a(i), blk.hex(' ')))

# --- 3. level data pointer tables ----------------------------------------
print('\n=== pointer tables into PRG $8000-$FFFF (sorted ascending runs) ===')
def is_ptr(i):
    lo, hi = prg[i], prg[i+1]
    v = lo | (hi << 8)
    return 0x8000 <= v <= 0xFFFF
runs2 = []
i = 0
while i < len(prg)-1:
    if is_ptr(i):
        st = i
        while i < len(prg)-1 and is_ptr(i):
            i += 2
        n = (i - st)//2
        if n >= 8:
            vals = [prg[st+k] | (prg[st+k+1] << 8) for k in range(0, i-st, 2)]
            mono = all(vals[k] <= vals[k+1] for k in range(len(vals)-1))
            print('%04X  n=%-3d monotonic=%s  %s' % (a(st), n, mono, ' '.join('%04X' % v for v in vals[:24])))
    else:
        i += 1

# --- 4. known SMB data regions -------------------------------------------
print('\n=== region entropy / byte stats per 1KB ===')
for k in range(0, 0x8000, 0x400):
    blk = prg[k:k+0x400]
    uniq = len(set(blk))
    zero = blk.count(0)/len(blk)
    ff = blk.count(0xFF)/len(blk)
    print('%04X uniq=%3d zero=%.2f ff=%.2f' % (a(k), uniq, zero, ff))
