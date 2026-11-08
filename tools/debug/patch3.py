p = 'C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s = open(p, encoding='latin-1').read()
marker = "        let brain = Brain::load(p).unwrap_or_else(|| {"
assert marker in s
start = s.index(marker)
end = s.index("        });", start) + len("        });")
new_block = "        let brain = if let Some(b) = Brain::load(p) { BrainKind::Old(b) }\n        else if let Some(b) = BrainV5::load(p) { BrainKind::V5(b) }\n        else {\n            eprintln!(\"Aviso: '{p}' aun no existe o es invalido; esperando hot-reload...\");\n            BrainKind::Old(Brain::fallback(idx as u64 + 1))\n        };"
s = s[:start] + new_block + s[end:]

old2 = "                        if let Some(b) = Brain::load(&a.loaded_path) {"
new2 = "                        if let Some(b) = Brain::load(&a.loaded_path) {\n                            a.brain = BrainKind::Old(b);"
assert old2 in s
s = s.replace(old2, new2, 1)
s = s.replace("                            a.brain = b;\n", "", 1)

open(p, 'w', encoding='latin-1').write(s)
print("done")
# end