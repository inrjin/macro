import assert from 'node:assert/strict';
import { loadAppPage } from './dom-fixture.mjs';

const field = 'legacy.session';
const initial = {
  phase: 'idle', runId: 1, revision: 0, captureState: 'idle', captureField: '', captureArmed: false,
  captureError: null, message: '', checks: [], manualActions: [], selectedCount: 0, targetCount: 2,
};
const state = (revision, captureState, message, captureError = null) => ({
  ...initial, revision, captureField: field, captureState, message, captureError,
  captureArmed: !['idle', 'captured'].includes(captureState),
});
const invoke = async (name, args, emit) => {
  assert.equal(name, 'arm_capture');
  const snapshot = state(1, 'armed', '회차 버튼 · 예매창의 대상 위에서 F8');
  emit('state-change', snapshot);
  return { field: args.field, profile: args.profile, profileRevision: 1, snapshot };
};
const main = await loadAppPage(new URL('../index.html', import.meta.url), initial, invoke);
const overlay = await loadAppPage(new URL('../overlay.html', import.meta.url), initial, invoke);
const $ = (selector) => {
  const found = main.document.querySelector(selector);
  assert.ok(found, `Actual HTML is missing ${selector}`);
  return found;
};
const overlayElement = (selector) => overlay.document.querySelector(selector);
const emit = (name, payload) => { main.emit(name, payload); overlay.emit(name, payload); };
assert.ok(main.listeners['runtime-error']?.length, 'Production runtime-error listener registered');
await $('[data-capture-field="legacy.session"]').click();
assert.equal(main.calls.at(-1).name, 'arm_capture');
assert.equal(main.calls.at(-1).args.field, field);
assert.match($('#capture-guide-title').textContent, /F8 입력 대기/);
emit('state-change', state(2, 'reading', 'F8 수신 · 회차 버튼 읽는 중'));
emit('state-change', state(3, 'reading', '회차 버튼 · 예매창 화면 읽는 중'));
assert.match($('#capture-guide-title').textContent, /화면 읽는 중/);
const message = '[target_not_foreground] 연결한 예매창이 맨 앞에 있지 않습니다.';
emit('runtime-error', { message, captureField: field });
emit('state-change', state(4, 'error', message, message));
assert.match($('#activity-log').firstElementChild.textContent, /회차 버튼 지정 실패.*target_not_foreground/);
assert.equal($('#activity-log').firstElementChild.classList.contains('error'), true);
assert.match($('#capture-guide-title').textContent, /회차 버튼 · 지정 실패/);
assert.match($('#capture-guide-message').textContent, /target_not_foreground.*F8/);
assert.match($('#runbar-status').textContent, /지정 실패/);
assert.equal($('[data-coordinate-field="legacy.session"]').classList.contains('capture-error'), true);
assert.equal($('[data-point-field="legacy.session"]').textContent, '미지정');
assert.equal($('#feedback').hidden, false);
assert.equal(overlayElement('#overlay').classList.contains('error'), true);
assert.match(overlayElement('#overlay-phase').textContent, /지정 실패.*target_not_foreground/);
assert.match(overlayElement('#overlay-message').textContent, /F8/);
// A clock/readiness update must not remove the error or flood the activity list.
const activityCount = $('#activity-log').children.length;
emit('state-change', { ...state(5, 'error', message, message), clock: { offsetMs: 12 } });
assert.equal($('#activity-log').children.length, activityCount);
assert.match($('#runbar-status').textContent, /지정 실패/);
// F8 remains armed; a retry can progress without clicking "F8로 지정" again.
emit('state-change', state(6, 'reading', 'F8 수신 · 회차 버튼 읽는 중'));
assert.equal($('#feedback').hidden, true);
assert.equal($('[data-coordinate-field="legacy.session"]').classList.contains('capture-error'), false);
emit('point-captured', {
  field, point: { x: 240, y: 360 }, color: { r: 10, g: 20, b: 30 },
  frame: { width: 1200, height: 800, scale_x: 2, scale_y: 2 },
});
emit('state-change', state(7, 'captured', '회차 버튼 지정 완료'));
assert.match($('[data-point-field="legacy.session"]').textContent, /240.*360/);
assert.match($('#save-status').textContent, /저장됨/);
assert.match($('#capture-guide-title').textContent, /지정 완료/);
assert.equal($('[data-coordinate-field="legacy.session"]').classList.contains('capture-error'), false);
assert.equal($('#feedback').hidden, true);
assert.equal(overlayElement('#overlay').classList.contains('error'), false);
// A newly opened popup reports the reconnect shortcut, even if the error event
// was missed while the main app was minimized.
const changed = '[target_window_changed] 연결된 예매창과 다른 창입니다.';
emit('state-change', state(8, 'error', changed, changed));
assert.match($('#capture-guide-message').textContent, /F7.*F8로 지정/);
assert.match(overlayElement('#overlay-message').textContent, /F7/);
// New native visibility errors must preserve the original reason while showing
// the specific recovery in both the main app and the non-focusing overlay.
const visibilityFailures = [
  ['accessibility_permission', /손쉬운 사용.*Integrated Macro.*허용/],
  ['target_hit_test_unavailable', /본문을 한 번 클릭.*F8.*F7/],
  ['target_focus_unavailable', /본문을 클릭하고 F7.*F8로 지정/],
  ['target_occluded', /창이나 메뉴를 닫거나 옮기고.*F8/],
];
for (const [index, [code, hint]] of visibilityFailures.entries()) {
  const original = `[${code}] 예매창 확인 실패 · 창 ID 123013`;
  emit('state-change', state(9 + index, 'error', original, original));
  assert.ok($('#capture-guide-message').textContent.includes(original));
  assert.ok($('#feedback').textContent.includes(original));
  assert.ok(overlayElement('#overlay-phase').textContent.includes(code));
  assert.equal(overlayElement('#overlay-message').title, original);
  assert.match($('#capture-guide-message').textContent, hint);
  assert.match(overlayElement('#overlay-message').textContent, hint);
}
console.log('PASS: Production app.js/overlay.js executed unchanged from actual HTML in isolated VM DOM.');
console.log('PASS: F8 arm → read → runtime error → persistent failure/overlay → retry → saved coordinate; missed error snapshot and no duplicate error activity.');
console.log('PASS: Accessibility, unavailable visibility, and occlusion errors retain native detail with specific recovery hints in app and overlay.');
console.log('Not verified: browser layout/rendering, Tauri bridge, macOS permissions/capture, or actual mouse input.');
