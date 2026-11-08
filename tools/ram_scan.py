import collections, json, sys
prg=open('analysis/prg.bin','rb').read()
print('prg len',len(prg))
print('vectors at end (NMI,RESET,IRQ as little endian):')
for name,off in [('NMI',-6),('RESET',-4),('IRQ',-2)]:
    lo,hi=prg[off],prg[off+1]
    print(f"  {name}: ${(hi<<8)|lo:04X}")
txt=open('analysis/disasm.txt',encoding='utf8',errors='ignore').read().splitlines()
print(f"disasm lines: {len(txt)}")
# show first 30
for l in txt[:30]:
    print(l)
print('--- most common $07xx accesses ---')
from collections import Counter
c=Counter()
for l in txt:
    parts=l.split()
    if len(parts)>=3:
        op=parts[2]
        operand=parts[3] if len(parts)>3 else ''
        if operand.startswith('$07'):
            c[operand]+=1
for k,v in c.most_common(40):
    print(k,v)
print('--- routines around key events ---')
for pat in ['$0750','$074F','$0739','$0722','$071D','$071B','$0756','$0757','$0758','$0759']:
    hits=[l for l in txt if pat in l][:8]
    print(f"== {pat} ==")
    for h in hits: print(" ",h)
