import codecs
data = open('C:/Users/USUARIO/Desktop/smb-rs/src/bin/watchall.rs','rb').read()
s = data.decode('utf-8', 'strict')
print('valid utf-8, len', len(s))
bad = [(i,c) for i,c in enumerate(s[:2000]) if ord(c) > 127]
print('non-ascii in first 2000:', bad[:5])
# end