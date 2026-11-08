prg = open('analysis/prg.bin','rb').read()
BASE=0x8000
def a(i): return BASE+i
def ptr(i): return prg[i] | (prg[i+1]<<8)

print('=== runs of pointers into $A000-$BFFF ===')
i=0
while i < len(prg)-1:
    v=ptr(i)
    if 0xA000 <= v < 0xC000:
        st=i; vals=[]
        while i < len(prg)-1:
            v=ptr(i)
            if 0xA000 <= v < 0xC000:
                vals.append(v); i+=2
            else: break
        if len(vals)>=4:
            print('%04X n=%-3d %s' % (a(st), len(vals), ' '.join('%04X'%v for v in vals)))
    else:
        i+=1

print()
print('=== runs of pointers into $8000-$FFFF that point forward > $9000 ===')
i=0
while i < len(prg)-1:
    v=ptr(i)
    if 0x9000 <= v < 0x10000 and v > a(i):
        st=i; vals=[]
        while i < len(prg)-1:
            v=ptr(i)
            if 0x9000 <= v < 0x10000 and v > a(i):
                vals.append(v); i+=2
            else: break
        if len(vals)>=8:
            print('%04X n=%-3d %s' % (a(st), len(vals), ' '.join('%04X'%v for v in vals)))
    else:
        i+=1
