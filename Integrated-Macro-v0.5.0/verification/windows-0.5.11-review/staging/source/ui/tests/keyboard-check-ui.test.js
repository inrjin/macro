import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { keyboardCheckView, normalizeSnapshot } from '../model.js';

test('keyboard diagnostic activity preserves the engine running state', () => {
  const snapshot = normalizeSnapshot({ phase: 'idle', running: false, keyboardCheck: {
    active: true, status: 'listening', remainingMs: 19_010,
    message: '연결한 Chrome에서 영문자를 입력하세요.', callbacks: 1, received: 1, accepted: 0,
  } });
  assert.equal(snapshot.running, false);
  assert.equal(snapshot.phase, 'idle');
  assert.equal(snapshot.label, '키보드 수신 확인 · 20초');
  assert.match(snapshot.message, /Chrome/);
  assert.equal(keyboardCheckView(snapshot).counts, 'OS 감지 1회 · 수신 1회 · 대상 확인 0회');
});

test('keyboard diagnostic results distinguish callbacks, acceptance, and unavailable permission evidence', () => {
  const view = keyboardCheckView({ keyboardCheck: { status: 'filtered', callbacks: 3, received: 1, accepted: 0,
    rejections: { 'callback:own_source': 2, 'runtime:window_mismatch': 1, 'queue:stale_event': 3, 'callback:invalid_event': -1 },
    environment: { platform: 'macos', accessibilityTrusted: true, listenEventAccess: false, secureEventInput: null },
  } });
  assert.equal(view.warning, true);
  assert.match(view.rejections, /입력 출처 제외 2회.*연결 창 불일치 1회.*입력 유효시간 초과 3회/);
  assert.doesNotMatch(view.rejections, /callback:|runtime:|queue:/);
  assert.doesNotMatch(view.rejections, /-1/);
  assert.match(view.environment, /손쉬운 사용: 허용.*입력 모니터링: 미허용.*보안 키 입력: 확인 불가/);
  assert.doesNotMatch(keyboardCheckView({ keyboardCheck: { status: 'received', accepted: 0 } }).label, /수신 확인됨/);
  assert.equal(keyboardCheckView({ keyboardCheck: { status: 'received', accepted: 1 } }).label, '키보드 수신 확인됨');
  assert.match(keyboardCheckView({ keyboardCheck: { environment: { platform: 'windows' } } }).environment,
    /Windows.*macOS 권한 항목은 적용되지 않습니다/);
  assert.equal(keyboardCheckView({ keyboardCheck: { callbacks: 0, received: 2, accepted: 1,
    environment: { platform: 'windows' } } }).counts, 'OS 감지: 제공 안 됨 · 수신 2회 · 대상 확인 1회');
});

test('finished keyboard results override earlier F8 feedback but yield to later operations', () => {
  const check = { active: false, status: 'received', accepted: 1, message: '키보드 수신이 확인되었습니다.' };
  const result = { phase: 'idle', running: false, message: check.message, keyboardCheck: check };
  assert.equal(keyboardCheckView(result).present, true);
  assert.equal(normalizeSnapshot(result).label, '키보드 수신 확인됨');
  assert.equal(keyboardCheckView({ ...result, message: '좌석 영역 위치 지정 완료' }).present, false);
  assert.equal(keyboardCheckView({ ...result, phase: 'seats', running: true }).present, false);
});

test('production keyboard diagnostic controls isolate checks from booking and show native results', () => {
  const result = spawnSync(process.execPath, [
    '--experimental-vm-modules', fileURLToPath(new URL('./run-keyboard-check-ui-dom.mjs', import.meta.url)),
  ], { encoding: 'utf8', timeout: 15000 });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
  assert.match(result.stdout, /PASS: Production keyboard diagnostic/);
});
