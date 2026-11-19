# Minimal UE5.1 legacy (cooked) package summary reader: names, imports, exports.
import struct, sys
class R:
    def __init__(s,b,o=0): s.b=b; s.o=o
    def i32(s): v=struct.unpack_from('<i',s.b,s.o)[0]; s.o+=4; return v
    def u32(s): v=struct.unpack_from('<I',s.b,s.o)[0]; s.o+=4; return v
    def i64(s): v=struct.unpack_from('<q',s.b,s.o)[0]; s.o+=8; return v
    def u64(s): v=struct.unpack_from('<Q',s.b,s.o)[0]; s.o+=8; return v
    def raw(s,n): v=s.b[s.o:s.o+n]; s.o+=n; return v
    def fstr(s):
        n=s.i32()
        if n==0: return ''
        if n>0: v=s.b[s.o:s.o+n-1].decode('latin1'); s.o+=n; return v
        n=-n; v=s.b[s.o:s.o+2*n-2].decode('utf-16le'); s.o+=2*n; return v
def load(path):
    b=open(path,'rb').read(); r=R(b)
    assert r.u32()==0x9E2A83C1
    leg=r.i32()
    if leg!=-4: r.i32()
    ue4=r.i32(); ue5=r.i32() if leg<=-8 else 0; lic=r.i32()
    if ue4==0: ue4=522; ue5=int(__import__("os").environ.get("UE5VER","1008"))
    n=r.i32(); r.raw(20*n)
    total=r.i32(); folder=r.fstr(); flags=r.u32()
    namecnt=r.i32(); nameoff=r.i32()
    if ue5>=1008: r.i32(); r.i32()   # soft object paths
    if not (flags & 0x80000000): r.fstr()  # localization id (not FilterEditorOnly)
    r.i32(); r.i32()  # gatherable text
    expcnt=r.i32(); expoff=r.i32(); impcnt=r.i32(); impoff=r.i32()
    # names
    r2=R(b,nameoff); names=[]
    for _ in range(namecnt):
        names.append(r2.fstr()); r2.raw(4)
    def nm(r): i=r.i32(); num=r.i32(); s=names[i]; return s if num==0 else f'{s}_{num-1}'
    r2=R(b,impoff); imps=[]
    for _ in range(impcnt):
        cp=nm(r2); cn=nm(r2); outer=r2.i32(); on=nm(r2)
        if ue4>=520 and not (flags & 0x80000000): nm(r2)  # package name
        if ue5>=1003: r2.i32() # bImportOptional
        imps.append(dict(cls=cn,outer=outer,name=on))
    r2=R(b,expoff); exps=[]
    for _ in range(expcnt):
        e={}
        e['cls']=r2.i32(); e['sup']=r2.i32(); e['tmpl']=r2.i32(); e['outer']=r2.i32(); e['name']=nm(r2)
        e['flags']=r2.u32(); e['size']=r2.i64(); e['off']=r2.i64()
        r2.raw(4*3)  # forced export, not for client, not for server
        if ue5<1005: r2.raw(16)
        if ue5>=1006: r2.raw(4)  # is inherited instance
        r2.raw(4*2)  # package flags, not always loaded for editor game
        r2.raw(4)    # is asset
        if ue5>=1003: r2.raw(4)  # generate public hash
        r2.raw(4*5)  # first export dependency + 4 counts
        exps.append(e)
    return dict(b=b,total=total,names=names,imps=imps,exps=exps,ue5=ue5,ue4=ue4)
def objname(p,idx):
    if idx<0: return p['imps'][-idx-1]['name']
    if idx>0: return p['exps'][idx-1]['name']
    return None
if __name__=='__main__':
    p=load(sys.argv[1])
    print('ue4',p['ue4'],'ue5',p['ue5'],'names',len(p['names']),'imps',len(p['imps']),'exps',len(p['exps']),'total',p['total'])
    for i,e in enumerate(p['exps'][:5]): print(i+1,objname(p,e['cls']),e['name'],objname(p,e['outer']),e['off'],e['size'])
