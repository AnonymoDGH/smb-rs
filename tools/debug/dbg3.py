p = 'C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s = open(p, encoding='latin-1').read()
old = 'let brain = if let Some(b) = Brain::load(p) { BrainKind::Old(b) }'
new = 'eprintln!("LOADING {}", p);\nlet brain = if let Some(b) = Brain::load(p) { eprintln!("  -> Old"); BrainKind::Old(b) }'
assert old in s
s = s.replace(old, new, 1)
open(p, 'w', encoding='latin-1').write(s)
print('done')
# end