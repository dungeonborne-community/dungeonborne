import sys,struct,math,re
sys.path.insert(0,'tools'); import pkg
def root_loc(d):
    o=0; frags=[]
    while True:
        h=struct.unpack_from('<H',d,o)[0]; o+=2
        skip=h&0x7f; zero=(h>>7)&1; last=(h>>8)&1; num=h>>9
        frags.append((skip,zero,num))
        if last: break
    zn=sum(f[2] for f in frags if f[1])
    first_zero=0
    if zn:
        ms=1 if zn<=8 else 2 if zn<=16 else ((zn+31)//32)*4
        mask=d[o:o+ms]; o+=ms
        if frags[0][1]: first_zero=mask[0]&1
    if frags[0][0]!=5 or frags[0][2]<1 or first_zero: return None
    return struct.unpack_from('<3d',d,o)
def sets(level):
    P='legacy/Dungeonborne/Plugins/GameFeatures/TgfMaps/Content/Scenes/GothicCastle/'+level
    p=pkg.load(P+'.umap'); uexp=open(P+'.uexp','rb').read()
    E=p['exps']; on=lambda i: pkg.objname(p,i)
    out={}
    for i,e in enumerate(E):
        if on(e['cls'])!='BP_LyraPlayerStartSet_C': continue
        for c in E:
            if c['outer']==i+1 and c['name']=='DefaultSceneRoot':
                o=c['off']-p['total']; out[e['name']]=root_loc(uexp[o:o+c['size']])
    return out
if __name__=='__main__':
    lvl=sys.argv[1]; census={}
    for line in open(sys.argv[2],encoding='utf-8',errors='replace'):
        m=re.search(r'(BP_LyraPlayerStartSet_C_UAID_\w+) team=(\S+) args=\[(.*?)\] wait=(\w+)',line)
        if m: census[m.group(1)]=(m.group(3),m.group(4))
    S=sets(lvl)
    rows=[]
    for n,l in sorted(S.items()):
        a,w=census.get(n,('?','?'))
        rows.append((n,l,a,w))
        print(n[-30:], 'args=[%s] wait=%s'%(a,w), ' '.join('%.0f'%v for v in l) if l else None)
    print('--- pairs closer than 15 m (combat sets only):')
    B=[r for r in rows if r[1] and r[3]!='true']
    for i in range(len(B)):
        for j in range(i+1,len(B)):
            d=math.dist(B[i][1],B[j][1])
            if d<1500: print('%6.0f cm'%d, B[i][0][-24:], '[%s]'%B[i][2], '<->', B[j][0][-24:], '[%s]'%B[j][2])
