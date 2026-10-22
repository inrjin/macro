from pathlib import Path
import datetime
import hashlib
import json
import plistlib
import re
import shutil
import subprocess
import tempfile
import zipfile

root = Path('/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0')
version = '0.5.9'
verification = root / 'verification'
distribution = root.parent / ('Integrated-Macro-macOS-v' + version)
archive = root.parent / (distribution.name + '.zip')
built_app = root / 'target/release/bundle/macos/Integrated Macro.app'
expected_version_changes = {
    'Cargo.lock', 'Cargo.toml', 'crates/macro-core/Cargo.toml',
    'crates/macro-platform/Cargo.toml', 'package-lock.json', 'package.json',
    'src-tauri/tauri.conf.json', 'ui/index.html', 'ui/package.json',
}

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def write_json(path, value):
    with path.open('x') as handle:
        json.dump(value, handle, ensure_ascii=False, indent=2)
        handle.write('\n')

def run(*args):
    result = subprocess.run(args, text=True, capture_output=True, check=True)
    return (result.stdout + result.stderr).strip()

def manifest(folder):
    assert not any(p.is_symlink() for p in folder.rglob('*')), 'Unexpected symbolic link in package'
    return {str(p.relative_to(folder)): {'sha256': sha(p), 'mode': p.stat().st_mode & 0o777}
            for p in sorted(folder.rglob('*')) if p.is_file()}

def source_files(folder):
    skipped = {'target', 'node_modules', 'verification', '.git', '.build-tools'}
    extensions = {'.rs', '.swift', '.js', '.mjs', '.toml', '.json', '.html', '.css',
                  '.h', '.c', '.sh', '.ps1', '.plist', '.lock', '.png', '.icns', '.ico', '.mask'}
    for path in folder.iterdir():
        if path.name in skipped:
            continue
        if path.is_dir():
            yield from source_files(path)
        elif path.suffix in extensions:
            yield str(path.relative_to(root))

def preservation():
    before = json.loads(Path('/private/tmp/macro-059-preservation-before.json').read_text())
    old_folder = Path(before['folder'])
    current = {str(p.relative_to(old_folder)): sha(p) for p in old_folder.rglob('*') if p.is_file()}
    assert current == before['files'], 'Previous release folder changed'
    assert sha(root.parent / 'Integrated-Macro-macOS-v0.5.8.zip') == before['zipSha256']
    assert sha(root.parent / 'AGENTS.md') == before['agentsSha256']
    return {'previousFolderFiles': len(current), 'previousFolderUnchanged': True,
            'previousZipSha256': before['zipSha256'], 'agentsSha256': before['agentsSha256']}

assert not distribution.exists(), 'Refusing to overwrite distribution'
assert not archive.exists(), 'Refusing to overwrite archive'
for name in ['local-checks-', 'build-artifacts-', 'distribution-receipt-']:
    assert not (verification / (name + version + '.json')).exists(), 'Refusing to overwrite release report'
checks_path = verification / 'local-checks.json'
checks = json.loads(checks_path.read_text())
for check in checks['checks']:
    if check.get('required'):
        assert check['status'] == 'passed', check['name']
by_name = {c['name']: c for c in checks['checks']}
for required in ['macOS native ABI verification', 'macOS packaged application',
                 'macOS application signature', 'Source stability before packaging',
                 'Source stability during verification']:
    assert by_name[required]['status'] == 'passed'
for name, digest in checks['sourceSha256'].items():
    assert sha(root / name) == digest, 'Source changed after build: ' + name
assert set(source_files(root)) == set(checks['sourceSha256']), 'Source file set changed after build'
audit = json.loads((verification / 'priority-audit/source-freeze.json').read_text())
changed = {name for name, digest in audit.items() if sha(root / name) != digest}
assert changed == expected_version_changes, changed
for name in expected_version_changes:
    text = (root / name).read_text()
    if name == 'Cargo.lock':
        for package in ['integrated-macro', 'macro-core', 'macro-platform']:
            new = 'name = "' + package + '"\nversion = "0.5.9"'
            assert text.count(new) == 1
            text = text.replace(new, new.replace('0.5.9', '0.5.8'))
    else:
        text = text.replace('0.5.9', '0.5.8')
    assert hashlib.sha256(text.encode()).hexdigest() == audit[name], 'Non-version change: ' + name
assert set(checks['sourceSha256']) == {name for name in audit if not name.endswith('.md')}
node_output = by_name['JavaScript behavior tests']['output']
js_count = int(re.search(r'(?:#|ℹ) tests (\d+)', node_output).group(1))
rust_output = by_name['Rust core, platform and desktop tests']['output']
rust_counts = [int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', rust_output)]
assert rust_counts
rust_count = sum(rust_counts)
assert (js_count, rust_count) == (141, 270), 'Review unexpected test count'
assert re.search(r'(?:#|ℹ) fail 0', node_output)
assert not re.search(r'test result: FAILED', rust_output)
preserved = preservation()

plist = plistlib.loads((built_app / 'Contents/Info.plist').read_bytes())
assert plist['CFBundleShortVersionString'] == version
assert plist['CFBundleVersion'] == version
assert plist['CFBundleIdentifier'] == 'dev.integrated-macro.desktop'
assert plist['LSMinimumSystemVersion'] == '13.0'
executable = built_app / 'Contents/MacOS/integrated-macro'
assert run('lipo', '-archs', str(executable)) == 'arm64'
load_commands = run('otool', '-l', str(executable))
assert re.findall(r'^\s*minos (\S+)', load_commands, re.MULTILINE) == ['13.0']
library_output = run('otool', '-L', str(executable))
libraries = [line.strip().split(' (compatibility version', 1)[0]
             for line in library_output.splitlines()[1:] if line.strip()]
assert libraries and all(lib.startswith(('/System/Library/', '/usr/lib/')) for lib in libraries), libraries
signature = run('codesign', '--verify', '--deep', '--strict', str(built_app))
signature_details = run('codesign', '-dv', '--verbose=4', str(built_app))
assert 'Signature=adhoc' in signature_details
built_manifest = manifest(built_app)

archived_checks = verification / ('local-checks-' + version + '.json')
assert not archived_checks.exists()
shutil.copy2(checks_path, archived_checks)
distribution.mkdir()
packaged_app = distribution / built_app.name
run('ditto', str(built_app), str(packaged_app))
assert manifest(packaged_app) == built_manifest
run('codesign', '--verify', '--deep', '--strict', str(packaged_app))
for name in ['input-check.html', 'keyboard-check.html']:
    shutil.copy2(root / 'fixtures' / name, distribution / name)
packaged_verification = distribution / 'verification'
packaged_verification.mkdir()
for name in ['local-checks-0.5.9.json', 'build-0.5.9.log',
             'multi-agent-audit.md', 'multi-agent-audit.json',
             'cancellation-audit.md', 'cancellation-audit.json',
             'keyboard-repair-review.md', 'keyboard-repair-review.json']:
    shutil.copy2(verification / name, packaged_verification / name)
shutil.copytree(verification / 'priority-audit', packaged_verification / 'priority-audit')

readme = '''통합 매크로 v0.5.9 · macOS 실험용 배포
Apple Silicon(arm64), macOS 13 이상

업데이트
1. 기존 앱에서 F4로 중지한 뒤 앱을 완전히 종료하세요.
2. 새 Integrated Macro.app을 기존에 사용하던 위치에 복사·교체하고 실행하세요.
3. 앱 화면의 v0.5.9를 확인하고, 저장된 옵션과 브라우저 연결을 확인하세요.
   구버전과 신버전을 동시에 실행하지 마세요.

앱 식별자와 설정 저장 경로는 유지합니다. 기존 0.5.8 배포 폴더와 ZIP은 보존했습니다.
앱은 로컬 ad-hoc 서명이며 Apple 공증을 받지 않았습니다. 업데이트 후 권한 자동 승계는
보장하지 않습니다. 입력·캡처가 실패할 때만 앱의 진단에 따라 시스템 설정의 손쉬운 사용,
화면 기록, 입력 모니터링 항목을 확인하고 앱을 다시 실행하세요. 일괄 권한 초기화는 필요 없습니다.

이번 수정
- macOS 키보드 수신 대상 판정과 첫 입력 전달, 앱 내 ‘키보드 수신 확인’.
- F8 좌표 지정, 설정 저장, 시작·중지의 비동기 충돌 처리.
- 한 구역/두 구역 취소표 설정과 새로고침 OFF 탐색 경로의 설정 적용.
- 일부 화면 배율에서 좌석 후보의 옆 픽셀을 읽던 좌표 변환.
기존 진입 버튼 순서, 첫 입력 기준 저장 방식, F9, 선택 추정 정책은 유지합니다.

실험 순서
1. 키보드 수신
   앱 ‘실행 → 브라우저 연결’로 대상 Chrome을 연결한 뒤 ‘키보드 수신 확인’을 누르세요.
   설정창이 내려가면 연결한 Chrome의 일반 입력란에서 영문 1~2자를 직접 입력하세요.
   약 20초 뒤 ‘키보드 수신 확인됨’과 ‘대상 확인 1회 이상’을 확인하세요.
   민감한 정보 대신 동봉한 keyboard-check.html을 Chrome에서 열어 검사해도 됩니다.
   검사 수치는 앱 수신 결과이며 입력한 글자 내용은 기록하지 않습니다.

2. 캡챠 이후 진행
   ‘추가 기능 → NOL 캡챠 판단’을 켜고 기존 예매 진입 설정으로 F1을 누르세요.
   진입 버튼 처리가 끝난 캡챠 대기 중 첫 영문자에는
   ‘NOL 캡챠 판단 · 첫 입력 기준 저장됨 · 배경 색 변화 대기’만 표시되어야 합니다.
   입력창이 남아 있을 때 구역·좌석을 클릭하면 실패입니다. 앞선 진입 클릭은 정상입니다.
   직접 정답 입력을 완료하여 창을 닫으면 구역 선택 또는 좌석 탐색으로 이어져야 합니다.
   계속 기다린다면 시각과 상태를 남기세요. 직접 캡챠를 완료한 뒤에만 F9로 이어갈 수 있습니다.
   F9 수동 진행 성공과 자동 종료 감지 성공은 구분해서 확인하세요.

3. 취소표 연속 탐색
   F4로 중지한 뒤 현재 좌석 화면에서 ‘좌석 · 구역 → 취소표 탐색 방식’을
   ‘한 구역 · 현재 좌석 계속 탐색’으로, ‘취소표에서 버튼 사용’을 OFF로 설정하세요.
   공통 좌석 색상·좌석 영역·선택 완료 버튼 설정을 확인하고 F2를 누르세요.
   빈 화면에서도 ‘빈 좌석 재탐색·새로고침 간격 · ms’의 두 주기 이상 탐색을 유지해야 합니다.
   이 설정에서는 새로고침 입력을 보내지 않습니다. 사이트가 새 좌석을 표시해야 찾을 수 있습니다.
   좌석이 보이면 후보 인식→클릭→선택 추정을 확인하고 사이트의 실제 선택과 비교하세요.
   F4로 중지한 뒤 동작이 다시 시작되지 않아야 합니다.

단축키
F1 일반 예매 · F2 취소표 탐색 · F3 일시정지/재개 · F4 중지
F5 좌석 바로 탐색(브라우저 새로고침 아님) · F6 원본 흐름 대기단계 확인
F7 브라우저 연결 · F8 좌표 지정 · F9 캡챠 이후 진행
Mac 키보드 설정에 따라 Fn과 함께 눌러야 할 수 있습니다.

검사와 남은 한계
JavaScript {js}개 + Rust {rust}개 = {total}개 통과, Swift arm64/x86_64 컴파일과
호스트 ABI 검사, Mac 앱 빌드·서명·소스 동일성 검사를 통과했습니다.
이 패키지의 앱은 arm64입니다. Swift x86_64 검사 통과는 Intel 앱 배포를 뜻하지 않습니다.
검사 통과에는 알려진 한계가 예상대로 재현되는 경우도 포함됩니다.
실제 예매창 입력·좌석 선택, Windows 배포, 실측 반응 속도는 이번 검사로 확인하지 않았습니다.

남은 한계: 구역 전환 중 이전 화면 구분, 늦은 실패 알림의 좌석 귀속,
같은 색 범례·붙거나 아주 작은 좌석의 오인/누락, 선택 추정과 사이트 실제 선택의 불일치.
오류창 없이 클릭을 무시하거나 기존 선택이 사라지면 앱이 이를 알지 못할 수 있습니다.
선택 완료 버튼 전달 이후 최종 성공 확인은 하지 않습니다.
자세한 근거: verification/priority-audit/README.md
빌드 기록: verification/local-checks-0.5.9.json, verification/build-0.5.9.log

문제가 생기면 실패 시각과 앱 ‘실행 기록’을 남기세요.
세부 기록: ~/Library/Application Support/dev.integrated-macro.desktop/runtime-events.jsonl
로그는 용량 제한이 있으므로 실패 직후 복사해 보존하는 편이 좋습니다.
'''.format(js=js_count, rust=rust_count, total=js_count + rust_count)
(distribution / 'README.txt').write_text(readme)
receipt = {
    'capturedAt': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'version': version, 'architecture': 'arm64', 'minimumMacOS': '13.0',
    'bundleIdentifier': plist['CFBundleIdentifier'], 'signature': 'ad-hoc; not notarized',
    'tests': {'javascript': js_count, 'rust': rust_count, 'total': js_count + rust_count},
    'sourceSha256': checks['sourceSha256'], 'sourceFiles': len(checks['sourceSha256']),
    'auditBaseline': 'priority-audit/source-freeze.json',
    'changesSinceFinalAudit': sorted(changed), 'changesAreVersionOnly': True,
    'checksReportSHA256': sha(archived_checks), 'appFiles': built_manifest,
    'signatureVerification': 'passed', 'builtAndPackagedAppIdentical': True,
    'machOMinimumMacOS': '13.0', 'dynamicLibraries': libraries,
    'dynamicLibrariesAreSystemOnly': True,
    'nativeVerification': 'Swift arm64/x86_64 compilation and host synthetic ABI; no actual input',
    'unverified': checks['unverified'], 'preservation': preserved,
    'appLaunched': False, 'userSettingsModified': False,
    'note': 'Tests include expected reproduction of documented limitations. This is an experimental release.'
}
write_json(packaged_verification / 'package-receipt.json', receipt)
distribution_manifest = manifest(distribution)
run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(distribution), str(archive))
with zipfile.ZipFile(archive) as zipped:
    assert zipped.testzip() is None
    names = zipped.namelist()
    assert all(n.startswith(distribution.name + '/') or n.startswith('__MACOSX/') for n in names)
    assert not any(n.endswith('/settings.json') or n.endswith('/runtime-events.jsonl') for n in names)
with tempfile.TemporaryDirectory(prefix='macro-059-zip-', dir='/private/tmp') as directory:
    run('ditto', '-x', '-k', str(archive), directory)
    unpacked = Path(directory) / distribution.name
    assert manifest(unpacked) == distribution_manifest, 'Archive changed content or file modes'
    run('codesign', '--verify', '--deep', '--strict', str(unpacked / built_app.name))
preservation()
assert set(source_files(root)) == set(checks['sourceSha256']), 'Source file set changed while packaging'
for name, digest in checks['sourceSha256'].items():
    assert sha(root / name) == digest, 'Source changed while packaging: ' + name
receipt.update({'distributionPath': str(distribution), 'archivePath': str(archive),
                'archiveSha256': sha(archive), 'archiveBytes': archive.stat().st_size,
                'archiveExtractedManifestMatches': True, 'extractedAppSignatureVerified': True})
write_json(verification / ('build-artifacts-' + version + '.json'), receipt)
write_json(verification / ('distribution-receipt-' + version + '.json'), {
    'version': version, 'archivePath': str(archive), 'archiveSha256': sha(archive),
    'archiveBytes': archive.stat().st_size, 'distributionFiles': distribution_manifest,
    'archiveExtractedManifestMatches': True, 'extractedAppSignatureVerified': True,
    'preservation': preserved,
})
print(json.dumps({k: receipt[k] for k in ['version', 'tests', 'sourceFiles', 'archivePath',
                  'archiveSha256', 'archiveBytes', 'archiveExtractedManifestMatches']}, indent=2))
