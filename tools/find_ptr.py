import sys
prg = open('analysis/prg.bin','rb').read()
BASE=0x8000
def a(i): return BASE+i

def ptr(i):
    return prg[i] | (prg[i+1]<<8)

print('=== monotonic 16-bit pointer runs (n>=6) ===')
i=0
found=[]
while i < len(prg)-1:
    v=ptr(i)
    if 0x8000 <= v <= 0xFFFF:
        st=i; vals=[]
        while i < len(prg)-1:
            v=ptr(i)
            if 0x8000 <= v <= 0xFFFF:
                vals.append(v); i+=2
            else: break
        if len(vals)>=6:
            mono=all(vals[k]<=vals[k+1] for k in range(len(vals)-1))
            found.append((st,vals,mono))
    else:
        i+=1

for st,vals,mono in found:
    if mono or len(vals)>=10:
        print('%04X n=%d mono=%s %s' % (a(st), len(vals), mono, ' '.join('%04X'%v for v in vals[:40])))

print()
print('=== non-monotonic runs (level pointer tables often not sorted) n>=12 ===')
for st,vals,mono in found:
    if not mono and len(vals)>=12 and len(set(vals))>8:
        print('%04X n=%d %s' % (a(st), len(vals), ' '.join('%04X'%v for v in vals[:40])))
