prg=open("analysis/prg.bin","rb").read(); B=0x8000
def dump(a,n=24):
    o=a-B
    print("%04X: %s"%(a," ".join("%02X"%b for b in prg[o:o+n])))
for a in (0xBC27,0xBC3F,0x8F40,0x8F97,0x8FA1,0x82F5,0x9C03):
    dump(a)
