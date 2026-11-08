txt=open('analysis/disasm.txt',encoding='utf8',errors='ignore').read().splitlines()
def dump(addr_str, before=6, after=80):
    idx=[i for i,l in enumerate(txt) if l.startswith(addr_str)]
    if not idx: print("NOT FOUND",addr_str); return
    for l in txt[idx[0]-before:idx[0]+after]:
        print(l)
for a in ['8223','81C6','90C5','9C22','C144','C164','B21B','B03E','90CC']:
    print(f"\n===== ${a} =====")
    dump(a, 4, 90)
