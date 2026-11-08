p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
old='    fn load(path: &str) -> Option<Self> {\n        let s = std::fs::read_to_string(path).ok()?;'
new='    fn load(path: &str) -> Option<Self> {\n        eprintln!("V5::load called for {}", path);\n        let s = std::fs::read_to_string(path).ok()?;'
assert old in s
s=s.replace(old,new,1)
open(p,'w',encoding='latin-1').write(s)
print('done')
# end