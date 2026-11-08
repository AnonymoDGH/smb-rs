code = b'const ROM_DEFAULT: &str =\n    r"C:\\Users\\USUARIO\\Downloads\\Super Mario Bros. (World)\\Super Mario Bros. (World).nes";\nfn main() { println!("{}", ROM_DEFAULT); }\n'
open('C:/Users/USUARIO/Desktop/mini2.rs','wb').write(code)
print(repr(code))
# end