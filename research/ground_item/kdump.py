"""Print BP bytecode from kismet-analyzer to-json: python kdump.py <asset.json> [function_filter]"""
import json, sys

d = json.load(open(sys.argv[1], encoding='utf-8'))
flt = sys.argv[2] if len(sys.argv) > 2 else None
ex = d['Exports']; imp = d['Imports']


def ref(i):
    try:
        i = int(i)
        if i > 0: return ex[i - 1]['ObjectName']
        if i < 0: return imp[-i - 1]['ObjectName']
    except Exception:
        return '?%s' % i
    return 'null'


def pp(p):
    if isinstance(p, dict):
        if 'New' in p and isinstance(p['New'], dict):
            path = p['New'].get('Path') or []
            return '.'.join(str(x) for x in path) or str(p['New'])
        if 'Old' in p:
            return ref(p['Old'])
    return str(p)


SKIP = {'$type', 'Tag', 'StatementIndex'}


def fmt(x):
    if isinstance(x, list):
        return '[' + ', '.join(fmt(v) for v in x) + ']'
    if not isinstance(x, dict):
        return json.dumps(x, ensure_ascii=False)
    t = x.get('$type', '').split(',')[0].split('.')[-1]
    if t in ('EX_LocalVariable', 'EX_InstanceVariable', 'EX_LocalOutVariable', 'EX_DefaultVariable'):
        return pp(x.get('Variable'))
    if t in ('EX_FinalFunction', 'EX_LocalFinalFunction', 'EX_CallMath'):
        return '%s(%s)' % (ref(x.get('StackNode')), ', '.join(fmt(a) for a in x.get('Parameters', [])))
    if t in ('EX_VirtualFunction', 'EX_LocalVirtualFunction'):
        return '%s(%s)' % (x.get('VirtualFunctionName'), ', '.join(fmt(a) for a in x.get('Parameters', [])))
    if t == 'EX_Context' or t == 'EX_Context_FailSilent':
        return '%s.%s' % (fmt(x.get('ObjectExpression')), fmt(x.get('ContextExpression')))
    if t in ('EX_Let', 'EX_LetObj', 'EX_LetBool', 'EX_LetWeakObjPtr', 'EX_LetValueOnPersistentFrame', 'EX_LetDelegate', 'EX_LetMulticastDelegate'):
        a = x.get('Variable') or x.get('VariableExpression') or x.get('DestinationProperty')
        return '%s = %s' % (fmt(a) if isinstance(a, dict) and '$type' in a else pp(a), fmt(x.get('Expression') or x.get('AssignmentExpression')))
    if t == 'EX_ObjectConst':
        return 'obj:' + ref(x.get('Value'))
    if t in ('EX_IntConst', 'EX_ByteConst', 'EX_FloatConst', 'EX_DoubleConst', 'EX_NameConst', 'EX_Int64Const'):
        return str(x.get('Value'))
    if t in ('EX_StringConst', 'EX_UnicodeStringConst'):
        return repr(x.get('Value'))
    if t == 'EX_True': return 'true'
    if t == 'EX_False': return 'false'
    if t in ('EX_Self',): return 'self'
    if t in ('EX_NoObject',): return 'None'
    if t == 'EX_StructMemberContext':
        return '%s.%s' % (fmt(x.get('StructExpression')), pp(x.get('StructMemberExpression')))
    if t in ('EX_DynamicCast', 'EX_MetaCast', 'EX_ObjToInterfaceCast', 'EX_CrossInterfaceCast', 'EX_InterfaceToObjCast'):
        return 'Cast<%s>(%s)' % (ref(x.get('ClassPtr')), fmt(x.get('Target')))
    if t == 'EX_JumpIfNot':
        return 'if not %s goto %s' % (fmt(x.get('BooleanExpression')), x.get('CodeOffset'))
    if t == 'EX_Jump':
        return 'goto %s' % x.get('CodeOffset')
    if t in ('EX_PopExecutionFlowIfNot',):
        return 'pop if not ' + fmt(x.get('BooleanExpression'))
    if t == 'EX_PushExecutionFlow':
        return 'push %s' % x.get('PushingAddress')
    if t == 'EX_ComputedJump':
        return 'goto ' + fmt(x.get('CodeOffsetExpression'))
    if t == 'EX_SoftObjectConst':
        return 'soft:' + fmt(x.get('Value'))
    if t in ('EX_Return', 'EX_Nothing', 'EX_EndOfScript', 'EX_PopExecutionFlow'):
        return t[3:]
    parts = []
    for k, v in x.items():
        if k in SKIP: continue
        parts.append('%s=%s' % (k, fmt(v) if isinstance(v, (dict, list)) else v))
    return '%s{%s}' % (t[3:] if t.startswith('EX_') else t, ', '.join(parts))


for e in ex:
    bc = e.get('ScriptBytecode')
    if not bc: continue
    name = e['ObjectName']
    if flt and flt not in name: continue
    print('=== %s' % name)
    for st in bc:
        print('  %5s  %s' % (st.get('StatementIndex', ''), fmt(st)))
