#!/usr/bin/env python3
"""Exercise the actual macOS binary's native API without Node or a GUI."""
import concurrent.futures
import http.cookiejar
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

binary = Path(sys.argv[1]).resolve()
root = Path(__file__).resolve().parents[1]
checks = []

def check(name, assertion):
    assert assertion, name
    checks.append(name)
    print('PASS', name, flush=True)

with tempfile.TemporaryDirectory(prefix='xcpc-tauri-') as tmp:
    env = dict(os.environ, XCPC_FONT_DIRS=str(root / '.fonts'))
    env.pop('XCPC_WORKSPACE', None)
    process = subprocess.Popen([str(binary), '--serve-test'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    try:
        ready = json.loads(process.stdout.readline())
        origin, token = ready['origin'], ready['token']
        def request(path, method='GET', body=None, auth=True, headers=None):
            h = dict(headers or {})
            if auth:
                h['x-xcpc-token'] = token
            data = None if body is None else json.dumps(body).encode()
            if data:
                h['content-type'] = 'application/json'
            req = urllib.request.Request(origin + path, data=data, method=method, headers=h)
            try:
                response = urllib.request.urlopen(req, timeout=180)
            except urllib.error.HTTPError as e:
                response = e
            payload = response.read()
            content_type = response.headers.get('content-type','')
            return response.status, json.loads(payload) if 'application/json' in content_type else payload, response.headers
        def api(path, method='GET', body=None):
            code, data, _ = request(path, method, body)
            assert code == 200, (path, code, data)
            return data
        check('reject unauthenticated requests', request('/api/workspace',auth=False)[0] == 403)
        check('reject foreign origins', request('/api/workspace',headers={'Origin':'https://example.com'})[0] == 403)
        cookies = http.cookiejar.CookieJar()
        browser = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cookies))
        page = browser.open(origin+'/?token='+token).read()
        check('browser cookie bootstrap and embedded UI', b'<div id="root">' in page and any(c.name=='xcpc_token' for c in cookies))
        check('workspace initially closed', api('/api/workspace')['root'] is None)
        check('compile without workspace returns structured error', request('/api/compile/problem/missing','POST',{})[:2] == (409, {'code':'no_workspace','message':'no workspace open'}))
        workspace = Path(tmp) / '中文 空间 + workspace'
        api('/api/workspace/example', 'POST', {'path':str(workspace),'language':'zh'})
        problems = api('/api/problems')
        contests = api('/api/contests')
        check('example library and contests', len(problems)>=3 and len(contests)>=1)
        generated_contest = api('/api/contests','POST',{'name':'出题验证赛'})
        saved = api('/api/setter/commit','POST',{
            'brief':{'background':'','tier':'warmup','tags':['math'],'notes':''},
            'draft':{
                'slug':'plus-one','title':'出题验证题','rating':800,'tags':['math','implementation'],
                'limits':{'timeMs':1000,'memoryMb':256},
                'background':'用于验证出题保存流程。','legend':'给定一个整数 $n$，请输出 $n+1$。',
                'input':'输入一个整数 $n$。','output':'输出 $n+1$。','constraints':'$1 \\le n \\le 10$',
                'samples':[{'input':'1\n','output':'2\n','explanation':'$1+1=2$。'}],
                'editorial':'直接输出 $n+1$。',
                'std':'#include <cstdio>\nint main(){int n;scanf("%d",&n);printf("%d\\n",n+1);}',
                'brute':'#include <cstdio>\nint main(){int n;scanf("%d",&n);printf("%d\\n",n+1);}',
                'generator':'#include <cstdio>\nint main(){puts("1");}',
                'wrong':[],
            },
            'report':None,
            'contest':generated_contest['id'],
        })
        generated_problem = api('/api/problems/'+urllib.parse.quote(saved['id']))
        generated = {'contest': api('/api/contests/'+urllib.parse.quote(generated_contest['id']))}
        check('setter saves a problem package with difficulty',
              saved['id']=='plus-one' and generated_problem['meta'].get('rating')==800
              and generated_problem['meta']['tags']==['math','implementation']
              and len(generated_problem['samples'])==1
              and generated_problem['statements'][0]['text'].startswith('## 题目背景')
              and '## 样例解释' in generated_problem['statements'][0]['text'])
        check('setter problem joins contest',
              any(item['item']['id']==generated_problem['id'] for item in generated['contest']['items']))
        generated_report = api('/api/compile/problem/'+urllib.parse.quote(generated_problem['id']),'POST',{})
        check('generated problem compiles without sample warning', generated_report.get('pages',0)>0 and
              not any('samples heading' in d.get('message','') for d in generated_report['diagnostics']))
        generated_contest_report = api('/api/compile/contest/'+urllib.parse.quote(generated_contest['id']),'POST',{})
        check('generated contest compiles without sample warning', generated_contest_report.get('pages',0)>0 and
              not any('samples heading' in d.get('message','') for d in generated_contest_report['diagnostics']))
        problem = api('/api/problems/'+urllib.parse.quote(problems[0]['id']))
        file_path = problem['statements'][0]['path']
        query = urllib.parse.urlencode({'path':file_path})
        before = api('/api/file?'+query)
        check('search and problem metadata', len(api('/api/search?q='+urllib.parse.quote(problems[0]['title']))) > 0 and 'limits' in problem['meta'])
        check('rating filter validates inputs', request('/api/problems?min=bad')[0] == 400)
        check('path escape blocked', 400 <= request('/api/file?path=../../etc/passwd')[0] < 500)
        escape = workspace / 'problems' / 'escape'
        escape.symlink_to(Path(tmp),target_is_directory=True)
        check('symlink escape blocked', 400 <= request('/api/file?path=problems/escape/outside.txt')[0] < 500)
        events=[]
        def listen():
            req = urllib.request.Request(origin+'/api/events',headers={'x-xcpc-token':token})
            with urllib.request.urlopen(req,timeout=15) as response:
                while True:
                    line=response.readline().decode().strip()
                    if line.startswith('data: '):
                        event=json.loads(line[6:]);events.append(event)
                        if event.get('type')=='changed':return
        thread=threading.Thread(target=listen,daemon=True);thread.start();time.sleep(.3)
        updated=api('/api/file','PUT',{'path':file_path,'text':before['text']+'\n\n修改验证\n','hash':before['hash']})
        check('atomic save updates hash', updated['hash'] != before['hash'])
        check('stale save conflict blocked',request('/api/file','PUT',{'path':file_path,'text':'overwrite','hash':before['hash']})[0] == 409)
        thread.join(8)
        check('workspace file events reach SSE clients',any(e.get('type')=='changed' for e in events))
        api('/api/file','PUT',{'path':file_path,'text':before['text'],'hash':updated['hash']})
        sample=problem['samples']
        got=api('/api/problems/'+problem['id']+'/samples','PUT',[{'input':s['input'],'output':s['output']} for s in sample])
        check('sample editing retains data',len(got)==len(sample))
        doc='/problem/'+urllib.parse.quote(problem['id'])
        report=api('/api/compile'+doc,'POST',{})
        check('native Typst problem compilation',report.get('pages',0)>0 and len(report['pageSizes'])==report['pages'])
        code,png,headers=request('/api/page'+doc+'/0?scale=1&rev='+str(report['rev']))
        check('native PNG preview',code==200 and png.startswith(b'\x89PNG\r\n\x1a\n'))
        Path(tmp,'page.png').write_bytes(png)
        code,pdf,headers=request('/api/pdf'+doc)
        check('native PDF publishing',code==200 and pdf.startswith(b'%PDF-'))
        cdoc='/contest/'+urllib.parse.quote(contests[0]['id'])
        cr=api('/api/compile'+cdoc,'POST',{})
        check('contest publishing and page map',cr.get('pages',0)>1 and len(cr['problems'])>0)
        location=api('/api/locate'+cdoc+'/0?x=100&y=100')
        check('source location endpoint',location is None or all(k in location for k in ('file','line','column')))
        disk=(workspace/file_path).read_bytes()
        overlay=api('/api/compile'+doc,'POST',{'unsaved':{'path':file_path,'text':before['text']+'\n\n未保存内容\n'}})
        check('unsaved preview leaves disk untouched',overlay.get('pages',0)>0 and (workspace/file_path).read_bytes()==disk)
        check('stale preview revisions rejected',request('/api/page'+doc+'/0?scale=1&rev='+str(report['rev']))[0]==409)
        current_rev=overlay['rev']
        api_pdf=request('/api/pdf'+doc)[1]
        check('disk-only export does not replace unsaved preview',api_pdf.startswith(b'%PDF-') and request('/api/page'+doc+'/0?scale=1&rev='+str(current_rev))[0]==200)
        bad=api('/api/compile'+doc,'POST',{'unsaved':{'path':file_path,'text':before['text']+'\n\n![](missing.png)\n'}})
        check('compile diagnostics preserve source path',any(d.get('file')==file_path for d in bad['diagnostics']))
        api('/api/workspace/open','POST',{'path':str(workspace),'readOnly':True})
        check('read-only flag',api('/api/workspace')['readOnly'])
        check('read-only writes blocked',request('/api/file','PUT',{'path':file_path,'text':'no'})[0]==403)
        api('/api/workspace/init','POST',{'path':str(Path(tmp)/'empty')})
        check('workspace switch clears library and sessions',api('/api/problems')==[] and request('/api/page'+doc+'/0?rev=1')[0]>=400)
        print(json.dumps({'passed':len(checks),'checks':checks},ensure_ascii=False,indent=2))
    finally:
        process.terminate()
        try: process.wait(timeout=10)
        except subprocess.TimeoutExpired: process.kill();process.wait()
