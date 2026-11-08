#!/usr/bin/env python3
"""Genera video MP4 desde out_*/*.png + telemetry.csv con overlay de velocidad e inputs."""
import csv, sys, os, glob
from pathlib import Path
import cv2  # opencv
from PIL import Image, ImageDraw, ImageFont
import numpy as np

OUTDIR = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("out_video")
VIDOUT = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("out_video/demo.mp4")

def load_tele():
    tele = {}
    with open(OUTDIR / "telemetry.csv") as f:
        for row in csv.DictReader(f):
            tele[int(row["frame"])] = row
    return tele

def draw_overlay(frame_pil, row):
    # frame_pil: 512x480 (scale2) -> we'll make overlay bar on top of padded video 512x560
    # Actually return composited image 512x560 with HUD
    W, H = frame_pil.size
    pad = 80
    canvas = Image.new("RGB", (W, H+pad), (0,0,0))
    canvas.paste(frame_pil, (0, pad))
    d = ImageDraw.Draw(canvas)
    # Try font
    try:
        font = ImageFont.truetype("C:/Windows/Fonts/arialbd.ttf", 12)
        font_small = ImageFont.truetype("C:/Windows/Fonts/arial.ttf", 11)
    except:
        font = ImageFont.load_default()
        font_small = font
    # Buttons bitmask: A 0x01, B 0x02, Select 0x04, Start 0x08, Up 0x10, Down 0x20, Left 0x40, Right 0x80
    btn = int(row["buttons"])
    names = [("A",0x01),("B",0x02),("SEL",0x04),("START",0x08),("UP",0x10),("DOWN",0x20),("LEFT",0x40),("RIGHT",0x80)]
    x = 6
    d.text((x, 4), f"Frame {int(row['frame'])}", fill=(255,255,0), font=font)
    x += 90
    for name, bit in names:
        # box
        pressed = (btn & bit) != 0
        col = (0,255,0) if pressed else (60,60,60)
        d.rectangle([x, 4, x+34, 18], outline=(255,255,255), fill=col if pressed else (30,30,30))
        d.text((x+4, 6), name, fill=(0,0,0) if pressed else (180,180,180), font=font_small)
        x += 38
    # second line: speed
    xspd = int(row["x_spd"]); yspd = int(row["y_spd"]); mf = int(row["x_moveforce"]); xabs = int(row["x_abs"])
    xpos = int(row["x_pos"]); ypos = int(row["y_pos"]); state = int(row["state"]); iv = int(row["interval"]); st = int(row["screentimer"])
    t = f"Xspd={xspd:+3d} (1/16px)  MoveForce={mf} (1/4096)  Xabs={xabs}  Yspd={yspd:+3d}  State={state}  X={xpos} Y={ypos}  21f={iv:02d} ScrT={st}  Btn=0x{btn:02X}"
    d.text((6, 24), t, fill=(0,255,255), font=font_small)
    # speed bar: X speed -60..60 -> 0..40 display (40 max)
    # background
    d.rectangle([6, 42, 250, 58], fill=(30,30,30), outline=(255,255,255))
    # needle: center for 0, right for positive, left for negative. Range -64..64 => 0..244 px
    cx = 6+122
    d.line([(cx,42),(cx,58)], fill=(255,255,0), width=1)
    pos = max(-64, min(64, xspd))
    nx = int(cx + (pos/64)*120)
    d.line([(cx,50),(nx,50)], fill=(0,255,0) if pos>=0 else (255,0,0), width=3)
    d.text((260, 42), f"vel px/f = {xspd/16.0:+.2f}  (MoveForce/4096={mf/64.0:.2f}/frame)", fill=(200,200,200), font=font_small)
    d.text((260, 58), "Trick: L+R 1 frame = brake instantaneo | Mantener L+R = andando sin avanzar (moonwalk parcial)", fill=(150,150,150), font=font_small)
    return canvas

def main():
    tele = load_tele()
    frames = sorted(glob.glob(str(OUTDIR / "frame_*.png")))
    if not frames:
        print("no frames"); return
    print(f"{len(frames)} frames")
    # get size
    first = Image.open(frames[0])
    W, H = first.size
    pad = 80
    out_w, out_h = W, H+pad
    fps = 60.0988
    VIDOUT.parent.mkdir(parents=True, exist_ok=True)
    fourcc = cv2.VideoWriter_fourcc(*"mp4v")
    writer = cv2.VideoWriter(str(VIDOUT), fourcc, fps, (out_w, out_h))
    if not writer.isOpened():
        print("failed to open writer"); return
    for i, fp in enumerate(frames):
        frame_num = int(Path(fp).stem.split("_")[1])
        row = tele.get(frame_num, {})
        img = Image.open(fp).convert("RGB")
        canvas = draw_overlay(img, row)
        # to cv2 BGR
        arr = np.array(canvas)
        bgr = cv2.cvtColor(arr, cv2.COLOR_RGB2BGR)
        writer.write(bgr)
        if i % 60 == 0:
            print(f"wrote {i}/{len(frames)} frame {frame_num}")
    writer.release()
    print(f"done {VIDOUT} size {VIDOUT.stat().st_size}")

if __name__ == "__main__":
    main()
