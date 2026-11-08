import json,struct
prg=open("analysis/prg.bin","rb").read()
base=0x8000
def b(a): return prg[a-base]
# world addr offsets table $9CB4, area addr offsets $9CBC per disasm labels
print("WorldAddrOffsets@9CB4:", [hex(b(0x9CB4+i)) for i in range(16)])
print("AreaAddrOffsets@9CBC:", [hex(b(0x9CBC+i)) for i in range(24)])
