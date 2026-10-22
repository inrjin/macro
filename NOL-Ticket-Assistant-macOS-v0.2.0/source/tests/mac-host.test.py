"""Native-host integration in an isolated temporary IPC directory; no browser registry changes."""
import json, os, pathlib, select, struct, subprocess, tempfile, time, uuid
host=pathlib.Path('macos/build/NOL Ticket Assistant.app/Contents/MacOS/NolTicketNativeHost').resolve()
def write_json(path,obj):
    temp=path.with_suffix('.tmp');temp.write_text(json.dumps(obj));temp.chmod(0o600);temp.replace(path)
def send(p,obj):
    data=json.dumps(obj,ensure_ascii=False).encode();p.stdin.write(struct.pack('<I',len(data))+data);p.stdin.flush()
def receive(p,timeout=2):
    if not select.select([p.stdout],[],[],timeout)[0]:raise AssertionError('native response timeout')
    header=p.stdout.read(4);assert len(header)==4
    length=struct.unpack('<I',header)[0];return json.loads(p.stdout.read(length))
checks=[]
with tempfile.TemporaryDirectory(prefix='nol-native-ipc-') as directory:
    root=pathlib.Path(directory);cmd={'id':str(uuid.uuid4()),'action':'seatStart','payload':{'browser':'chrome'}}
    write_json(root/'command.json',cmd)
    p=subprocess.Popen([str(host),'--test-root',directory],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    try:
        send(p,{'op':'ping'});assert receive(p)['ok'];checks.append('real framed ping')
        assert not select.select([p.stdout],[],[],.3)[0];checks.append('old command never replayed')
        cmd['id']=str(uuid.uuid4());write_json(root/'command.json',cmd)
        reply=receive(p);assert reply['id']==cmd['id'] and reply['op']=='appCommand';checks.append('new command delivered')
        assert not select.select([p.stdout],[],[],.25)[0];checks.append('command delivered once')
        send(p,{'op':'status','state':{'browser':'chrome','seatRun':{'phase':'DONE'},'profile':{'color':'#8b5cf6'},'logs':['한글 기록']}})
        target=root/'status-chrome.json'
        deadline=time.monotonic()+2
        while not target.exists() and time.monotonic()<deadline:time.sleep(.01)
        status=json.loads(target.read_text());assert status['seatRun']['phase']=='DONE' and status['_receivedAt']>0 and (target.stat().st_mode&0o777)==0o600;checks.append('private status and receipt timestamp')
        write_json(root/'command.json',{'id':str(uuid.uuid4()),'action':'exec','payload':{}})
        assert not select.select([p.stdout],[],[],.25)[0];checks.append('unknown command rejected')
        p.stdin.close();assert p.wait(timeout=3)==0;checks.append('clean EOF exit')
    finally:
        if p.poll() is None:p.kill();p.wait()
    p=subprocess.Popen([str(host),'--test-root',directory],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    p.stdin.write(struct.pack('<I',65537));p.stdin.flush();assert p.wait(timeout=3)==1;checks.append('oversized native frame rejected')
result={'ok':True,'checks':checks,'scope':'compiled universal native host on Apple Silicon; temporary directory; actual Chrome registration not changed'}
pathlib.Path('verification').mkdir(exist_ok=True)
pathlib.Path('verification/mac-host-results.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result,indent=2))
