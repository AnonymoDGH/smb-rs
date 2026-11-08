p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
old='    let mut agents: Vec<Agent> = Vec::new();'
new='    eprintln!("paths={:?}", paths);\n    let mut agents: Vec<Agent> = Vec::new();'
assert old in s
s=s.replace(old,new,1)
open(p,'w',encoding='latin-1').write(s)
print('done')
# end