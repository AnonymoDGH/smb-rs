p='C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s=open(p, encoding='latin-1').read()
old = s[s.find('let brain = Brain::load(p)')-8:s.find('let brain = Brain::load(p)')+130]
new = """        let brain = if let Some(b) = Brain::load(p) { BrainKind::Old(b) }
        else if let Some(b) = BrainV5::load(p) { BrainKind::V5(b) }
        else {
            eprintln!("Aviso: '{p}' aun no existe o es invalido; esperando hot-reload...");
            BrainKind::Old(Brain::fallback(idx as u64 + 1))
        };"""
s=s.replace(old,new,1)

old2 = "                        if let Some(b) = Brain::load(&a.loaded_path) {"
new2 = "                        if let Some(b) = Brain::load(&a.loaded_path) {\n                            a.brain = BrainKind::Old(b);"
assert old2 in s
s=s.replace(old2,new2,1)
s=s.replace("                            a.brain = b;\n", "", 1)

open(p,'w', encoding='latin-1').write(s)
print("done")
"