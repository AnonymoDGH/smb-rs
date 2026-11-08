p = 'C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s = open(p, encoding='latin-1').read()
old = "        let brain = Brain::load(p).unwrap_or_else(|| {\n            eprintln!(\"Aviso: '{p}' a" + "un no existe o es inv" + "álido; esperando hot-reload...\");\n            Brain::fallback(idx as u64 + 1)\n        });"
new = ("        let brain = if let Some(b) = Brain::load(p) { BrainKind::Old(b) }\n"
       "        else if let Some(b) = BrainV5::load(p) { BrainKind::V5(b) }\n"
       "        else {\n"
       "            eprintln!(\"Aviso: '{p}' a" + "un no existe o es inv" + "álido; esperando hot-reload...\");\n"
       "            BrainKind::Old(Brain::fallback(idx as u64 + 1))\n"
       "        };")
assert old in s, "block1 not found"
s = s.replace(old, new, 1)

old2 = "                        if let Some(b) = Brain::load(&a.loaded_path) {"
new2 = "                        if let Some(b) = Brain::load(&a.loaded_path) {\n                            a.brain = BrainKind::Old(b);"
assert old2 in s, "block2 not found"
s = s.replace(old2, new2, 1)
s = s.replace("                            a.brain = b;\n", "", 1)

open(p, 'w', encoding='latin-1').write(s)
print("done")
"