p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
data=open(p,'rb').read()
data=data.replace(b'\r\n', b'\n')
open(p,'wb').write(data)
print('converted to LF')
# end