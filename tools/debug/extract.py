data = open('C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs','rb').read()
lines = data.split(b'\n')
# lines 46-47 (0-indexed 45,46) - the const ROM_DEFAULT block
snippet = b'const ROM_DEFAULT: &str =\n' + lines[45] + b'\nfn main() { println!("{}", ROM_DEFAULT); }\n'
open('C:/Users/USUARIO/Desktop/ministrip.rs','wb').write(snippet)
print(repr(snippet))
# end