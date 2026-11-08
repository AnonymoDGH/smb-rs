p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
s=s.replace('let a = s.find(chr(34)+chr(39).replace(chr(39),chr(34))+'w'+chr(34)+chr(39).replace(chr(39),chr(34))+chr(34)+\':\').map(|i| i + 4)?;', 'let a = s.find("\\"w\\":").map(|i| i + 4)?;')
# simpler: just replace the whole chr line
import re
s=s.replace('        let a = s.find(chr(34)+\'w\'+chr(34)+\':\').map(|i| i + 4)?;', '        let a = s.find("\\"w\\":").map(|i| i + 4)?;')
open(p,'w',encoding='latin-1').write(s)
print('done')
# end