import struct,sys,json
def load(path):
    b=open(path,'rb').read(); p=0
    def u32():
        nonlocal p; v=struct.unpack_from('<I',b,p)[0]; p+=4; return v
    def i32():
        nonlocal p; v=struct.unpack_from('<i',b,p)[0]; p+=4; return v
    def fs():
        nonlocal p; n=i32()
        if n==0: return ''
        if n<0:
            s=b[p:p-2*n].decode('utf-16-le'); p+=-2*n
        else:
            s=b[p:p+n].decode('latin-1'); p+=n
        return s.rstrip('\0')
    p=16; ver=b[p]; p+=1
    off=struct.unpack_from('<q',b,p)[0]; p+=8
    if ver>=2: u32()
    q=p; p=off; n=u32(); strs=[]
    for _ in range(n):
        strs.append(fs())
        if ver>=3: i32()
    p=q; out={}
    for _ in range(u32()):
        if ver>=2: u32()
        ns=fs()
        for _ in range(u32()):
            if ver>=2: u32()
            k=fs(); u32(); idx=i32()
            out[(ns,k)]=strs[idx]
    return out
if __name__=='__main__':
    d=load(sys.argv[1]); print(len(d))
