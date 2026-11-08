p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
old='    fn load(path: &str) -> Option<Self> {\n        let s = std::fs::read_to_string(path).ok()?;\n        let a = s.find'
new='    fn load(path: &str) -> Option<Self> {\n        eprintln!("V5LOAD {}", path);\n        let s = std::fs::read_to_string(path).ok()?;\n        let a = s.find'
assert old in s, 'not found'
s=s.replace(old,new,1)
open(p,'w',encoding='latin-1').write(s)
print('done')
# end