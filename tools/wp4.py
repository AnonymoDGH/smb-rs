prg=open("analysis/prg.bin","rb").read(); B=0x8000
def b(a): return prg[a-B]
for area in range(1,5):
    ap=b(0x9CBC+0x13+(area-1))
    t=(ap&0x60)>>5; idx=ap&0x1f
    yy=((ap&0x60)<<1)&0xff
    for _ in range(3): yy=((yy<<1)&0xff)|(yy>>7)
    yy&=0xff
    base=(b(0x9CE0+yy)+idx)&0xff
    el=b(0x9CE4+base); eh=b(0x9D06+base)
    lb=b(0x9D2C+((b(0x9D28+t)+idx)&0xff)); lh=b(0x9D4E+((b(0x9D28+t)+idx)&0xff))
    print("4-%d ap=%02X type=%d idx=%d enemy=%04X level=%04X"%(area,ap,t,idx,eh<<8|el,lh<<8|lb))
    a=eh<<8|el
    print("   ", " ".join("%02X"%b(a+i) for i in range(24)))
