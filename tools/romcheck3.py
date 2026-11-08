prg=open("analysis/prg.bin","rb").read(); B=0x8000
o=0x8F20-B
print(" ".join("%02X"%b for b in prg[o:o+40]))
