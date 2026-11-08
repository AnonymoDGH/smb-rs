p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
idx=0
for _ in range(2):
    i=s.find('fn load(path: &str) -> Option<Self> {', idx)
    print('--- at', i)
    print(repr(s[i:i+280]))
    idx=i+1
# end