p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
needle = chr(34)+'--weights'+chr(34)
i = s.find(needle)
print('found:', i)
if i>=0:
    print(repr(s[i-5:i+80]))
# end