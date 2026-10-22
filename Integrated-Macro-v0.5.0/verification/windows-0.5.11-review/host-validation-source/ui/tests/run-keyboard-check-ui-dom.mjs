import assert from 'node:assert/strict';
import { defaultProfile } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

const initial = {
  phase: 'idle', running: false, paused: false, runId: 0, revision: 0,
  captureState: 'captured', captureField: 'legacy.next', captureArmed: false, checks: [], manualActions: [],
  selectedCount: 0, targetCount: 2, keyboardCheck: { active: false, status: 'idle' },
};
const profile = defaultProfile();
// A diagnostic must not save/validate booking options to start.
profile.desktop.scheduled_start = true;
profile.desktop.sale_time = null;
let revision = 0;
let main;
let overlay;
let failStart = false;
const checking = {
  active: true, status: 'listening', remainingMs: 20_000,
  message: '연결한 Chrome에서 영문자를 직접 입력하세요.',
  callbacks: 0, received: 0, accepted: 0, rejections: {},
  environment: { platform: 'macos', accessibilityTrusted: true, listenEventAccess: false, secureEventInput: false },
};
function emit(check, changes = {}) {
  const next = { ...initial, revision: ++revision, keyboardCheck: check, message: check.message, ...changes };
  main.emit('state-change', next);
  overlay.emit('state-change', next);
  return next;
}
async function invoke(name, args) {
  if (name === 'start_keyboard_check') {
    if (failStart) throw new Error('키보드 검사: 손쉬운 사용 권한을 확인하세요.');
    return emit(checking);
  }
  if (name === 'start_shortcut') {
    assert.equal(args.profile, null);
    assert.match(args.error, /키보드 검사를 중지/);
    return { cancelled: true };
  }
  assert.equal(name, 'stop_keyboard_check');
  return emit({ ...checking, active: false, remainingMs: 0, status: 'cancelled', message: '키보드 검사를 중지했습니다.' });
}
main = await loadAppPage(new URL('../index.html', import.meta.url), initial, invoke, profile);
overlay = await loadAppPage(new URL('../overlay.html', import.meta.url), initial, invoke, profile);
const $ = (selector) => {
  const result = main.document.querySelector(selector);
  assert.ok(result, `Missing actual UI element ${selector}`);
  return result;
};
const overlayText = (id) => overlay.document.querySelector(id).textContent;
assert.equal($('#start-keyboard-check').disabled, false);
assert.equal($('#stop-keyboard-check').disabled, true);
await $('#start-keyboard-check').click();
assert.deepEqual(main.calls.filter(({ name }) => name !== 'get_bootstrap').map(({ name }) => name), ['start_keyboard_check']);
assert.equal($('#start-keyboard-check').disabled, true);
assert.equal($('#stop-keyboard-check').disabled, false);
assert.equal($('#stop-button').disabled, false);
assert.equal($('#pause-button').disabled, true);
assert.equal($('#status-symbol').classList.contains('running'), false);
assert.equal($('#mode-pill').textContent, '키보드 검사');
assert.match($('#phase-label').textContent, /키보드 수신 확인.*20초/);
assert.match($('#keyboard-check-environment').textContent, /손쉬운 사용: 허용.*입력 모니터링: 미허용/);
assert.match(overlayText('#overlay-phase'), /키보드 수신 확인.*20초/);
assert.equal(overlayText('#overlay-count'), '0회');

for (const id of ['start-button', 'save-button', 'import-button', 'bind-target', 'capture-color']) {
  assert.equal($(`#${id}`).disabled, true, `${id} must not run during a keyboard check`);
  await $(`#${id}`).click();
}
for (const control of main.document.querySelectorAll('[data-capture-field], [data-setting], .clear-point')) {
  assert.equal(control.disabled, true, 'Capture/configuration controls must stay locked, including zone colors');
}
assert.equal(main.calls.filter(({ name }) => name !== 'get_bootstrap').length, 1);
main.emit('shortcut-start-requested', { requestId: 1, mode: 'normal' });
await new Promise((resolve) => setTimeout(resolve, 0));
assert.equal(main.calls.at(-1).name, 'start_shortcut');

emit({ ...checking, status: 'filtered', remainingMs: 12_001, callbacks: 1,
  rejections: { 'callback:own_source': 1, 'queue:stale_event': 1, 'runtime:window_mismatch': 1 },
  message: '입력 출처 확인 단계에서 제외되었습니다.' });
assert.match($('#keyboard-check-counts').textContent, /OS 감지 1회.*수신 0회.*대상 확인 0회/);
assert.match($('#keyboard-check-rejections').textContent, /입력 출처 제외 1회.*입력 유효시간 초과 1회.*연결 창 불일치 1회/);
assert.equal($('#keyboard-check-rejections').hidden, false);
assert.equal($('#stop-keyboard-check').disabled, false);
assert.match(overlayText('#overlay-phase'), /13초/);

emit({ ...checking, status: 'received', remainingMs: 10_000, callbacks: 2, received: 1, accepted: 1 });
assert.equal($('#keyboard-check-status').textContent, '키보드 수신 확인됨');
assert.equal(overlayText('#overlay-count'), '1회');
assert.equal($('#start-button').disabled, true, 'A successful sample does not finish an active 20-second check');
emit({ ...checking, status: 'received', remainingMs: 10_000, callbacks: 0, received: 1, accepted: 1,
  environment: { platform: 'windows', accessibilityTrusted: null, listenEventAccess: null, secureEventInput: null } });
assert.match($('#keyboard-check-counts').textContent, /OS 감지: 제공 안 됨.*수신 1회.*대상 확인 1회/);
assert.match(overlayText('#overlay-message'), /OS 감지: 제공 안 됨/);
await $('#stop-button').click();
assert.equal(main.calls.at(-1).name, 'stop_keyboard_check');
assert.equal($('#stop-keyboard-check').disabled, true);
assert.equal($('#start-keyboard-check').disabled, false);
assert.equal($('#start-button').disabled, false);
assert.equal($('#save-button').disabled, false);
assert.equal($('#keyboard-check-status').textContent, '키보드 검사 중지됨');

await $('#start-keyboard-check').click();
await $('#stop-keyboard-check').click();
assert.equal(main.calls.at(-1).name, 'stop_keyboard_check');
await $('#start-keyboard-check').click();
emit({ ...checking, active: false, status: 'no_input', remainingMs: 0, message: '20초 동안 입력이 수신되지 않았습니다.' });
assert.equal($('#keyboard-check-status').textContent, '입력이 수신되지 않았습니다');
assert.equal($('#start-keyboard-check').disabled, false);
assert.equal($('#stop-keyboard-check').disabled, true);
assert.equal($('#status-symbol').classList.contains('running'), false);
assert.equal(overlayText('#overlay-phase'), '입력이 수신되지 않았습니다');
assert.equal(overlay.document.querySelector('#overlay-count').hidden, false);

failStart = true;
await $('#start-keyboard-check').click();
assert.match($('#feedback').textContent, /키보드 검사.*손쉬운 사용/);
assert.equal($('#start-keyboard-check').disabled, false);
assert.equal($('#stop-keyboard-check').disabled, true);
assert.equal(main.calls.some(({ name }) => ['save_profile', 'start_run', 'arm_capture', 'bind_target'].includes(name)), false);

const finished = { ...checking, active: false, status: 'received', remainingMs: 0,
  callbacks: 2, received: 1, accepted: 1, message: '키보드 수신이 확인되었습니다.' };
for (const captureState of ['captured', 'error']) {
  emit(finished, { captureState, captureField: 'legacy.next', captureArmed: false,
    captureError: captureState === 'error' ? '[target_occluded] 이전 좌표 지정 실패' : null });
  assert.equal(overlayText('#overlay-phase'), '키보드 수신 확인됨', `Old ${captureState} feedback must not hide the check result`);
  assert.equal(overlayText('#overlay-count'), '1회');
  assert.equal($('#phase-label').textContent, '키보드 수신 확인됨');
  assert.equal($('#capture-guide').hidden, true);
}
emit(finished, { captureState: 'armed', captureField: 'vision.roi.start', captureArmed: true,
  captureError: null, message: '좌석 영역 · 첫 모서리 · 예매창의 대상 위에서 F8' });
assert.equal(overlayText('#overlay-phase'), 'F8 입력 대기', 'A later capture takes priority over an earlier keyboard result');
assert.equal($('#capture-guide').hidden, false);
emit(finished, { phase: 'seats', running: true, captureState: 'idle', captureError: null,
  message: '빈 좌석 계속 탐색 중' });
assert.doesNotMatch(overlayText('#overlay-phase'), /키보드 수신/);
console.log('PASS: Production keyboard diagnostic UI locks booking controls, preserves stop, and distinguishes native results.');
console.log('Not verified: native keyboard delivery, TCC permissions, or CAPTCHA completion.');
