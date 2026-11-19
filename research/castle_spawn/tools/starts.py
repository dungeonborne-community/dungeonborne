import sys,struct
sys.path.insert(0,'tools'); import pkg
P='legacy/Dungeonborne/Plugins/GameFeatures/TgfMaps/Content/Scenes/GothicCastle/'+sys.argv[1]
p=pkg.load(P+'.umap'); uexp=open(P+'.uexp','rb').read()
E=p['exps']; on=lambda i: pkg.objname(p,i)
def data(e): o=e['off']-p['total']; return uexp[o:o+e['size']]
acts=[(i+1,e) for i,e in enumerate(E) if 'PlayerStart' in e['name'] and on(e['cls']) and 'Component' not in on(e['cls'])]
for idx,e in acts[:3]:
    print(idx,on(e['cls']),e['name'],'size',e['size'])
    kids=[(j+1,c) for j,c in enumerate(E) if c['outer']==idx]
    for j,c in kids: print('   ',j,on(c['cls']),c['name'],c['size'], data(c)[:96].hex())
print(len(acts))
