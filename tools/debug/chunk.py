p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
needle = chr(34)+'--weights'+chr(34)
i=s.find(needle)
chunk=s[i:i+200]
open('C:/Users/USUARIO/Desktop/chunk.txt','w').write(chunk)
# end