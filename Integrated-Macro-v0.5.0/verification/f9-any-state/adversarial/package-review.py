"""Read-only release inspection. Runs no packaged executable or GUI application."""
import argparse
import hashlib
import json
import plistlib
import re
import stat
import subprocess
import tempfile
import zipfile
from datetime import datetime, timezone
from html.parser import HTMLParser
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument('--distribution', required=True, type=Path)
parser.add_argument('--archive', required=True, type=Path)
parser.add_argument('--check-report', required=True, type=Path)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def command(argv):
    result = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, check=False)
    return {'argv': argv, 'exitCode': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}

def tree(path):
    result = {}
    for item in sorted(path.rglob('*')):
        key = str(item.relative_to(path))
        if item.is_symlink():
            result[key] = {'type': 'symlink', 'target': str(item.readlink())}
        elif item.is_file():
            result[key] = {'type': 'file', 'sha256': sha(item.read_bytes()), 'bytes': item.stat().st_size,
                'mode': oct(stat.S_IMODE(item.stat().st_mode))}
    return result

class Tokens(HTMLParser):
    def __init__(self, text):
        super().__init__(convert_charrefs=True)
        self.tokens = []
        self.feed(text)
    def handle_decl(self, value): self.tokens.append(('decl', value.lower()))
    def handle_starttag(self, tag, attrs): self.tokens.append(('start', tag, sorted((k, v or '') for k, v in attrs)))
    def handle_startendtag(self, tag, attrs): self.handle_starttag(tag, attrs)
    def handle_endtag(self, tag): self.tokens.append(('end', tag))
    def handle_data(self, value):
        value = re.sub(r'\s+', ' ', value).strip()
        if value: self.tokens.append(('text', value))
    def handle_comment(self, value): self.tokens.append(('comment', value))

issues = []
distribution = args.distribution.resolve()
app = distribution / 'Integrated Macro.app'
if not app.is_dir():
    apps = list(distribution.glob('*.app'))
    if len(apps) != 1: raise SystemExit('Expected exactly one application')
    app = apps[0]
plist = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
exe = app / 'Contents/MacOS' / plist['CFBundleExecutable']
commands = [command(argv) for argv in [
    ['file', str(exe)], ['lipo', '-archs', str(exe)], ['otool', '-L', str(exe)],
    ['codesign', '--verify', '--deep', '--strict', '--verbose=2', str(app)],
    ['codesign', '-dvvv', str(app)], ['codesign', '-d', '--entitlements', '-', str(app)],
]]
for c in commands:
    if c['exitCode']: issues.append('Inspection command failed: ' + ' '.join(c['argv']))
dependencies = [line.strip().split(' (')[0] for line in commands[2]['stdout'].splitlines()[1:] if line.strip()]
non_system = [x for x in dependencies if not x.startswith(('/System/Library/', '/usr/lib/'))]
if non_system: issues.append('Non-system dynamic dependency')
expected = tree(distribution)
build_app = ROOT / 'target/release/bundle/macos/Integrated Macro.app'
build_app_files = tree(build_app)
packaged_app_files = tree(app)
if not build_app_files or packaged_app_files != build_app_files: issues.append('Packaged app differs from build output')
with zipfile.ZipFile(args.archive) as z:
    entries = z.infolist()
    names = [x.filename for x in entries]
    unsafe = [name for name in names if PurePosixPath(name).is_absolute() or '..' in PurePosixPath(name).parts
        or '\\' in name or not PurePosixPath(name).parts
        or PurePosixPath(name).parts[0] not in (distribution.name, '__MACOSX')]
    for item in entries:
        if stat.S_ISLNK(item.external_attr >> 16):
            target = z.read(item).decode('utf-8')
            if PurePosixPath(target).is_absolute() or '..' in PurePosixPath(target).parts:
                unsafe.append(item.filename + ' -> ' + target)
    duplicates = sorted({name for name in names if names.count(name) > 1})
    corrupt = z.testzip()
    if unsafe or duplicates or corrupt: raise SystemExit('Unsafe, duplicate or corrupt archive entries')
with tempfile.TemporaryDirectory(prefix='macro-0511-review-') as temporary:
    extract = command(['ditto', '-x', '-k', str(args.archive.resolve()), temporary])
    if extract['exitCode']: raise SystemExit('ZIP extraction failed: ' + extract['stderr'])
    actual = tree(Path(temporary) / distribution.name)
    missing = sorted(set(expected) - set(actual))
    added = sorted(set(actual) - set(expected))
    different = sorted(p for p in expected.keys() & actual.keys() if expected[p] != actual[p])
    signature = command(['codesign', '--verify', '--deep', '--strict', '--verbose=2', str(Path(temporary) / distribution.name / app.name)])
    if missing or added or different or signature['exitCode']: issues.append('Extracted package differs or signature invalid')

node = r'''
const fs=require('fs'),path=require('path'),zlib=require('zlib'),crypto=require('crypto');
const binary=fs.readFileSync(process.argv[1]),all=[];
for(const n of fs.readdirSync('target/release/build')){
 if(!n.startsWith('integrated-macro-'))continue;
 const d=path.join('target/release/build',n,'out/tauri-codegen-assets');if(!fs.existsSync(d))continue;
 for(const f of fs.readdirSync(d)){
  const p=path.join(d,f),raw=fs.readFileSync(p);let plain;try{plain=zlib.brotliDecompressSync(raw)}catch{continue}
  const offset=binary.indexOf(raw);if(offset>=0)all.push({path:p,raw,plain,offset});
 }
}
const result=[];
for(const n of fs.readdirSync('ui')){
 const p=path.join('ui',n);if(!fs.statSync(p).isFile())continue;const src=fs.readFileSync(p);
 let m=all.filter(a=>a.plain.equals(src));
 if(n.endsWith('.html')){const title=n==='index.html'?'통합 매크로':'통합 매크로 상태';m=all.filter(a=>a.path.endsWith('.html')&&a.plain.toString().includes(`<title>${title}</title>`))}
 const seen=new Set();m=m.filter(a=>{const h=crypto.createHash('sha256').update(a.raw).digest('hex');if(seen.has(h))return false;seen.add(h);return true});
 result.push({source:p,matches:m.map(a=>({asset:a.path,compressedSHA256:crypto.createHash('sha256').update(a.raw).digest('hex'),plainSHA256:crypto.createHash('sha256').update(a.plain).digest('hex'),binaryOffset:a.offset,exact:src.equals(a.plain),html:n.endsWith('.html')?a.plain.toString():undefined}))});
}
process.stdout.write(JSON.stringify(result));
'''
res = subprocess.run(['node', '-e', node, str(exe)], cwd=ROOT, capture_output=True, text=True, check=True)
assets = json.loads(res.stdout)
for asset in assets:
    for match in asset['matches']:
        if 'html' in match:
            built = match.pop('html')
            match['normalizedHTMLTokensEqual'] = Tokens(built).tokens == Tokens((ROOT / asset['source']).read_text()).tokens
            if asset['source'] == 'ui/index.html': match['expectedVersionPresent'] = '0.5.11' in built
    if not any(m['exact'] or m.get('normalizedHTMLTokensEqual') for m in asset['matches']):
        issues.append('Embedded UI source mismatch: ' + asset['source'])
checks = json.loads(args.check_report.read_text())
mismatches = [name for name, value in checks['sourceSha256'].items() if not (ROOT/name).exists() or sha((ROOT/name).read_bytes()) != value]
if mismatches: issues.append('Source differs from build-gate fingerprint')
if not all(c['status']=='passed' for c in checks['checks'] if c.get('required')): issues.append('Required build check failed')
if plist.get('CFBundleShortVersionString') != '0.5.11': issues.append('Unexpected application version')
if plist.get('CFBundleVersion') != '0.5.11': issues.append('Unexpected bundle build version')
if plist.get('LSMinimumSystemVersion') != '13.0': issues.append('Unexpected minimum macOS version')
if not (exe.stat().st_mode & 0o111): issues.append('Application binary has no executable permission')
if plist.get('CFBundleIdentifier') != 'dev.integrated-macro.desktop': issues.append('Application identifier changed')
if commands[1]['stdout'].strip() != 'arm64': issues.append('Unexpected architecture')
report = {'recordedAt':datetime.now(timezone.utc).isoformat(),'issues':issues,'archive':str(args.archive),
    'archiveSHA256':sha(args.archive.read_bytes()),'archiveBytes':args.archive.stat().st_size,
    'archiveEntries':len(entries),'duplicateEntries':duplicates,'unsafeEntries':unsafe,'corruptEntry':corrupt,
    'distribution':str(distribution),'distributionFiles':expected,'packagedAppMatchesBuild':packaged_app_files == build_app_files,'extraction':{'missing':missing,'added':added,'different':different,'signature':signature},
    'infoPlist':plist,'commands':commands,'nonSystemDependencies':non_system,'assets':assets,
    'checkReport':str(args.check_report),'checkReportSHA256':sha(args.check_report.read_bytes()),
    'checkedSourceCount':len(checks['sourceSha256']),'sourceMismatches':mismatches,
    'limitations':['No app launch, actual capture/input/overlay, authenticated site or Windows runtime execution.','Thin arm64 application; x86_64 native compile is not an Intel app.','Ad-hoc local signature is not Apple notarization.','Embedded compressed bytes/HTML token comparison does not establish live webview behavior.']}
args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'issues':issues,'files':len(expected),'assets':len(assets),'archiveSHA256':report['archiveSHA256']},ensure_ascii=False))
if issues: raise SystemExit(1)
