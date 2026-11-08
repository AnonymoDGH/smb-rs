prg=open("analysis/prg.bin","rb").read(); B=0x8000
def b(a): return prg[a-B]
print("1-1 enemy 9F01:", " ".join("%02X"%b(0x9F01+i) for i in range(32)))
# try world4: WorldAddrOffsets[3]=0x13; areas 1..4 -> 9CBC[0x13..0x16]
for k in range(4):
    ap=b(0x9CBC+0x13+k); print("4-%d ap=%02X"%(k+1,ap))
