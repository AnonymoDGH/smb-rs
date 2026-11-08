prg=open("analysis/prg.bin","rb").read(); B=0x8000
def b(a): return prg[a-B]
def dump(a,n=48): print("%04X:"%a," ".join("%02X"%b(a+i) for i in range(n)))
dump(0x9F7B,64); print("level A232:"); dump(0xA232,32)
# find row0e entries in enemy stream, walking 2 bytes at a time until FF-ish
a=0x9F7B
for i in range(0,200,2):
    v=b(a+i)
    if (v&0x0f)==0x0e:
        print("row0e @%04X: %02X %02X %02X"%(a+i,v,b(a+i+1),b(a+i+2)))
