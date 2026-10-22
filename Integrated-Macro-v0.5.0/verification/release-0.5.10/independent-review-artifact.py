"""Read-only application inspection; writes only a new review JSON beside this script."""
import hashlib
import json
import plistlib
import re
import stat
import subprocess
from datetime import datetime, timezone
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / 'target/release/bundle/macos/Integrated Macro.app'
OUT = Path(__file__).with_suffix('.json')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def command(argv):
    p = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, check=False)
    return {'argv': argv, 'exitCode': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr}


class Tokens(HTMLParser):
    def __init__(self, text):
        super().__init__(convert_charrefs=True)
        self.tokens = []
        self.feed(text)

    def handle_decl(self, decl):
        self.tokens.append(('decl', decl.lower()))

    def handle_starttag(self, tag, attrs):
        attrs = sorted((k, v or '') for k, v in attrs)
        self.tokens.append(('start', tag, attrs))

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)

    def handle_endtag(self, tag):
        self.tokens.append(('end', tag))

    def handle_data(self, data):
        data = re.sub(r'\s+', ' ', data).strip()
        if data:
            self.tokens.append(('text', data))

    def handle_comment(self, text):
        self.tokens.append(('comment', text))


plist = plistlib.loads((APP / 'Contents/Info.plist').read_bytes())
exe = APP / 'Contents/MacOS' / plist['CFBundleExecutable']
commands = [command(a) for a in [
    ['file', str(exe)],
    ['lipo', '-archs', str(exe)],
    ['otool', '-L', str(exe)],
    ['otool', '-l', str(exe)],
    ['codesign', '--verify', '--deep', '--strict', '--verbose=2', str(APP)],
    ['codesign', '-dvvv', str(APP)],
    ['codesign', '-d', '--entitlements', '-', str(APP)],
]]
node = r'''
const fs=require('fs'), path=require('path'), zlib=require('zlib'), crypto=require('crypto');
const binary=fs.readFileSync(process.argv[1]);
const all=[];
for(const name of fs.readdirSync('target/release/build')) {
 if(!name.startsWith('integrated-macro-'))continue;
 const dir=path.join('target/release/build',name,'out/tauri-codegen-assets');
 if(!fs.existsSync(dir))continue;
 for(const f of fs.readdirSync(dir)) {
  const p=path.join(dir,f), raw=fs.readFileSync(p); let plain;
  try{plain=zlib.brotliDecompressSync(raw)}catch{continue}
  const offset=binary.indexOf(raw);
  if(offset>=0)all.push({path:p,raw,plain,offset});
 }
}
const result=[];
for(const name of fs.readdirSync('ui')) {
 const p=path.join('ui',name); if(!fs.statSync(p).isFile())continue;
 const src=fs.readFileSync(p);
 let matches=all.filter(a=>a.plain.equals(src));
 if(name.endsWith('.html')) {
  const title=name==='index.html'?'통합 매크로':'통합 매크로 상태';
  matches=all.filter(a=>a.path.endsWith('.html')&&a.plain.toString().includes(`<title>${title}</title>`));
 }
 const seen=new Set();
 matches=matches.filter(a=>{const h=crypto.createHash('sha256').update(a.raw).digest('hex');if(seen.has(h))return false;seen.add(h);return true});
 result.push({source:p,matches:matches.map(a=>({
  compressedAsset:a.path,compressedSHA256:crypto.createHash('sha256').update(a.raw).digest('hex'),
  decompressedSHA256:crypto.createHash('sha256').update(a.plain).digest('hex'),
  binaryOffset:a.offset,compressedSize:a.raw.length,decompressedSize:a.plain.length,
  exactSourceBytes:a.plain.equals(src),html:name.endsWith('.html')?a.plain.toString():undefined
 }))});
}
process.stdout.write(JSON.stringify(result));
'''
asset_process = subprocess.run(['node', '-e', node, str(exe)], cwd=ROOT, capture_output=True, text=True, check=True)
assets = json.loads(asset_process.stdout)
for asset in assets:
    for match in asset['matches']:
        if 'html' in match:
            built_html = match.pop('html')
            source_html = (ROOT / asset['source']).read_text()
            match['normalizedHTMLTokensEqual'] = Tokens(source_html).tokens == Tokens(built_html).tokens
            match['version0510Present'] = '0.5.10' in built_html if asset['source'] == 'ui/index.html' else None
        else:
            match['normalizedHTMLTokensEqual'] = None
report_path = ROOT / 'verification/local-checks.json'
checks = json.loads(report_path.read_text())
before = json.loads((ROOT / 'verification/release-0.5.10/source-before-build.json').read_text())
source_mismatches = [p for p, expected in before.items() if sha((ROOT / p).read_bytes()) != expected]
build_mismatches = [p for p, expected in checks['sourceSha256'].items() if sha((ROOT / p).read_bytes()) != expected]
dependencies = [line.strip().split(' (')[0] for line in commands[2]['stdout'].splitlines()[1:] if line.strip()]
non_system_dependencies = [d for d in dependencies if not d.startswith(('/System/Library/', '/usr/lib/'))]
result = {
    'generatedAt': datetime.now(timezone.utc).isoformat(),
    'reviewer': '/root/fix_cancellation_start',
    'app': str(APP.relative_to(ROOT)),
    'appFiles': {str(p.relative_to(APP)): {'sha256': sha(p.read_bytes()), 'bytes': p.stat().st_size, 'mode': oct(stat.S_IMODE(p.stat().st_mode))} for p in sorted(APP.rglob('*')) if p.is_file()},
    'infoPlist': plist,
    'commands': commands,
    'nonSystemDependencies': non_system_dependencies,
    'assets': assets,
    'assetMethod': 'Brotli-decompress codegen cache and match current UI source, then locate exact compressed payload in the final Mach-O. HTML compares parser tokens after whitespace and boolean-attribute normalization; no UI rendering was executed.',
    'sourceBeforeBuildCount': len(before), 'sourceBeforeBuildMismatches': source_mismatches,
    'checkReport': str(report_path.relative_to(ROOT)), 'checkReportSHA256': sha(report_path.read_bytes()),
    'checkReportFingerprintCount': len(checks['sourceSha256']), 'checkReportSourceMismatches': build_mismatches,
    'allRequiredChecksPassed': all(c.get('status') == 'passed' for c in checks['checks'] if c.get('required')),
    'limitations': ['No application launch or actual macOS input/capture/overlay test.', 'No live site or Windows runtime test.', 'Thin arm64 app; x86_64 native compile verification does not create an Intel application.', 'Ad-hoc signature, no Apple notarization.', 'HTML structural comparison and payload inclusion do not prove runtime webview rendering.'],
}
result['issues'] = []
for i, c in enumerate(commands):
    if c['exitCode'] != 0:
        result['issues'].append(f'Inspection command {i} failed')
if source_mismatches or build_mismatches:
    result['issues'].append('Source fingerprints changed')
if non_system_dependencies:
    result['issues'].append('Non-system dynamic dependencies need review')
for asset in assets:
    if not any(m['exactSourceBytes'] or m['normalizedHTMLTokensEqual'] for m in asset['matches']):
        result['issues'].append(f"Embedded source mismatch: {asset['source']}")
OUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({'report': str(OUT.relative_to(ROOT)), 'issues': result['issues'], 'UIAssets': len(assets), 'sourceCount': len(before), 'arch': commands[1]['stdout'].strip(), 'version': plist['CFBundleShortVersionString']}, ensure_ascii=False))
