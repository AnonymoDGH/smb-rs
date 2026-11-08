prg=open("analysis/prg.bin","rb").read(); B=0x8000
def b(a): return prg[a-B]
for start,n in [(0x9CE0,16),(0x9CE4,40),(0x9D06,40),(0x9D28,16),(0x9D2C,40),(0x9D4E,40)]:
    print("%04X:"%start, " ".join("%02X"%b(start+i) for i in range(n)))
