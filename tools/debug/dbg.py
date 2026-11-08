p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
old='        if w.len() != NW_V5 { return None; }'
new='        if w.len() != NW_V5 { eprintln!("V5 load: len={} expected={}", w.len(), NW_V5); return None; }'
assert old in s
s=s.replace(old,new,1)
open(p,'w',encoding='latin-1').write(s)
print('done')
# end