p = 'C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs'
s = open(p, encoding='latin-1').read()
old = '    r"C:\\Users\\USUARIO\\Downloads\\Super Mario Bros. (World)\\Super Mario Bros. (World).nes";'
new = '    "C:\\\\Users\\\\USUARIO\\\\Downloads\\\\Super Mario Bros. (World)\\\\Super Mario Bros. (World).nes";'
assert old in s, 'not found'
s = s.replace(old, new, 1)
open(p, 'w', encoding='latin-1').write(s)
print('done')
# end