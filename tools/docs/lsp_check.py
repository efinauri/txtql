import os, subprocess, json, sys
BIN=os.path.join(os.path.dirname(os.path.abspath(__file__)),'..','..','target','release','txtql')
p=subprocess.Popen([BIN,'lsp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE)
def send(m):
    b=json.dumps(m).encode(); p.stdin.write(b'Content-Length: %d\r\n\r\n'%len(b)+b); p.stdin.flush()
def recv():
    h=b''
    while not h.endswith(b'\r\n\r\n'): h+=p.stdout.read(1)
    n=int([l for l in h.split(b'\r\n') if l.lower().startswith(b'content-length')][0].split(b':')[1])
    return json.loads(p.stdout.read(n))
def req(i,method,params):
    send({'jsonrpc':'2.0','id':i,'method':method,'params':params})
    while True:
        m=recv()
        if m.get('id')==i: return m
send({'jsonrpc':'2.0','id':1,'method':'initialize','params':{'capabilities':{},'processId':None,'rootUri':None}})
r=recv(); caps=r['result']['capabilities']
print('capabilities:', sorted(k for k,v in caps.items() if v))
send({'jsonrpc':'2.0','method':'initialized','params':{}})
txt="-- the greeting\nTEXT = greet ' ' w:WORD\ngreet = 'hi' OR wrd\n"
uri='file:///q.tql'
send({'jsonrpc':'2.0','method':'textDocument/didOpen','params':{'textDocument':{'uri':uri,'languageId':'txtql','version':1,'text':txt}}})
m=recv()
print('diagnostics:', [(d['code'], d['severity']) for d in m['params']['diagnostics']])
h=req(2,'textDocument/hover',{'textDocument':{'uri':uri},'position':{'line':1,'character':9}})
print('hover:', json.dumps(h['result'])[:160])
d=req(3,'textDocument/definition',{'textDocument':{'uri':uri},'position':{'line':1,'character':9}})
print('definition:', d['result'])
rf=req(4,'textDocument/references',{'textDocument':{'uri':uri},'position':{'line':1,'character':9},'context':{'includeDeclaration':True}})
print('references:', len(rf['result']))
rn=req(5,'textDocument/rename',{'textDocument':{'uri':uri},'position':{'line':1,'character':9},'newName':'hello'})
print('rename edits:', json.dumps(rn['result'])[:200])
c=req(6,'textDocument/completion',{'textDocument':{'uri':uri},'position':{'line':2,'character':8}})
items=c['result'] if isinstance(c['result'],list) else c['result']['items']
print('completion count:', len(items), [i['label'] for i in items][:8])
o=req(7,'textDocument/documentSymbol',{'textDocument':{'uri':uri}})
print('outline:', [s['name'] for s in o['result']])
t=req(8,'textDocument/semanticTokens/full',{'textDocument':{'uri':uri}})
print('semantic tokens data length:', len(t['result']['data']))
p.kill()
