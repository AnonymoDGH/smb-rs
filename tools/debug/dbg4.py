p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
old='    fn load(path: &str) -> Option<Self> {\n        let s = std::fs::read_to_string(path).ok()?;'
# this pattern appears twice (Brain and BrainV5); target the second by finding BrainV5 impl
i=s.find('impl BrainV5 {')
j=s.find('fn load(path: &str) -> Option<Self> {', i)
old2=s[j:j+70]
new2=old2.replace('let s = std::fs::read_to_string(path).ok()?;', 'eprintln!("V5LOAD path={}", path);\n        let s = std::fs::read_to_string(path).ok()?;')
s=s.replace(old2,new2,1)
open(p,'w',encoding='latin-1').write(s)
print('done')
# end