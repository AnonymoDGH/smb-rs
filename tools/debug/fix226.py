p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
lines=open(p,'rb').read().split(b'\n')
# line 226 (index 225) is broken; line 47 (index 46) is the raw string
print('L47:', repr(lines[46]))
print('L226:', repr(lines[225]))
# fix L226
lines[225] = b'            .find("\\"fitness\\":")'
open(p,'wb').write(b'\n'.join(lines))
print('fixed')
# end