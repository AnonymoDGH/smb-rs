prg=open("analysis/prg.bin","rb").read(); B=0x8000
def b(a): return prg[a-B]
def p(a): return b(a)|(b(a+1)<<8)
for area in range(1,5):
    Y=0x13+(area-1)
    ap=b(0x9CBC+Y)
    t=(ap&0x60)>>5
    idx=ap&0x1f
    yy=((ap&0x60)<<1)
    for _ in range(3): yy=((yy<<1)&0xff)|(yy>>7)
    yy&=0xff
    e=p(0x9CE4+((b(0x9CE0+yy)+idx)&0xff)) if False else None
    base=(b(0x9CE0+yy)+idx)&0xff
    el=b(0x9CE4+base); eh=b(0x9D06+base)
    lb=b(0x9D2C+((b(0x9D28+t)+idx)&0xff)); lh=b(0x9D4E+((b(0x9D28+t)+idx)&0xff))
    print("4-%d ap=%02X type=%d idx=%d enemy=%04X level=%04X"%(area,ap,t,idx,eh<<8|el,lh<<8|lb))
    # scan enemy stream for row 0e
    a=eh<<8|el
    for i in range(0,300):
        v=b(a+i)
        if (v&0x0f)==0x0e:
            print("   row0e at %04X: %02X %02X %02X"%(a+i,v,b(a+i+1),b(a+i+2)))
