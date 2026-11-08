prg=open("analysis/prg.bin","rb").read(); B=0x8000
def b(a): return prg[a-B]
for t in range(8):
    print(t, [hex(b(0x9CE0+t*4+i)) for i in range(4)])
print("9CE0 raw", [hex(b(0x9CE0+i)) for i in range(16)])
print("9E4?", [hex(b(0x9CE4+i)) for i in range(20)])
