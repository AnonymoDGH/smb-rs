from collections import Counter
txt=open('analysis/disasm.txt',encoding='utf8',errors='ignore').read().splitlines()
c=Counter()
for l in txt:
    # format: ADDR  BYTES  MNEM OPERAND...
    parts=l.split()
    if len(parts)<3: continue
    # parts[0]=addr, parts[1]=first byte? actually bytes column splits
    # operand is last token if contains $
    for tok in parts:
        if tok.startswith('$07'):
            c[tok]+=1
for k,v in c.most_common(50):
    print(k,v)
print('--- $07xx writers near flag / level code ---')
import re
for l in txt:
    if '$0722' in l or '$0725' in l or '$0729' in l or '$0730' in l:
        print(l)
print('--- NMI / main loop ---')
# find NMI at 8082
idx=[i for i,l in enumerate(txt) if l.startswith('8082')]
print(idx)
for l in txt[idx[0]:idx[0]+90]:
    print(l)
