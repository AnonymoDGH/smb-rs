p = 'C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s = open(p, encoding='latin-1').read()
old = '''                            if let Some(b) = Brain::load(&a.loaded_path) {
                            a.brain = BrainKind::Old(b);'''
new = '''                            if let Some(b) = Brain::load(&a.loaded_path) {
                                a.brain = BrainKind::Old(b);
                            } else if let Some(b) = BrainV5::load(&a.loaded_path) {
                                a.brain = BrainKind::V5(b);'''
assert old in s
s = s.replace(old, new, 1)
# remove the now-duplicate closing that follows (the original had a.brain = b;)
s = s.replace("                                a.brain = b;\n", "", 1)
open(p, 'w', encoding='latin-1').write(s)
print('done')
# end