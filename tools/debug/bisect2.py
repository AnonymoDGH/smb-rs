import subprocess
data = open('C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs','rb').read()
lines = data.split(b'\n')
# try incremental heads
for n in [20, 26, 27, 30, 32, 34, 36, 38, 40, 42, 44, 45]:
    head = b'\n'.join(lines[:n]) + b'\nfn main() {}\n'
    open('C:/Users/USUARIO/Desktop/t_head.rs','wb').write(head)
    r = subprocess.run(['rustc','--edition','2021','C:/Users/USUARIO/Desktop/t_head.rs','-o','C:/Users/USUARIO/Desktop/t_head.exe'], capture_output=True)
    err = r.stderr.decode('utf-8','replace').splitlines()
    first = [l for l in err if 'error' in l.lower()]
    print(f'--- head n={n}: {first[0] if first else "OK"}')
# end