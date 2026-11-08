p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
old = '        let a = s.find(chr(34)+\'w\'+chr(34)+\':\').map(|i| i + 4)?;'
new = '        let a = s.find("\\"w\\":").map(|i| i + 4)?;'
assert old in s, 'not found'
s=s.replace(old,new,1)
open(p,'w',encoding='latin-1').write(s)
print('done')
# end