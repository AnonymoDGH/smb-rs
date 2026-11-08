p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
s=s.replace('        eprintln!("V5::load called for {}", path);\n', '', 1)
s=s.replace('eprintln!("LOADING {}", p);\n', '', 1)
open(p,'w',encoding='latin-1').write(s)
print('done')
# end