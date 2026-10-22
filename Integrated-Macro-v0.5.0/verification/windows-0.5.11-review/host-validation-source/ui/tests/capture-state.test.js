import test from 'node:test';
import assert from 'node:assert/strict';
import { captureRecoveryHint, captureStateFromError, captureStateFromSnapshot } from '../capture-state.js';

const field = 'legacy.session';
const armed = { field, status: 'armed', detail: '', armed: true };
const error = '[target_not_foreground] 연결한 예매창이 맨 앞에 있지 않습니다.';

test('F8 failure remains visible when a legacy armed snapshot arrives after the error event', () => {
  const failed = captureStateFromError(armed, { message: error });
  const state = captureStateFromSnapshot(failed, { captureField: field, captureArmed: true, message: error });
  assert.equal(state.status, 'error');
  assert.equal(state.detail, error);
  assert.equal(state.armed, true);
});

test('bootstrap snapshot restores a failure even when its runtime-error event was missed', () => {
  const state = captureStateFromSnapshot({ field: null, status: 'idle', detail: '', armed: false }, {
    captureField: field, captureArmed: true, captureState: 'error', captureError: error,
  });
  assert.equal(state.field, field);
  assert.equal(state.status, 'error');
  assert.equal(state.detail, error);
  assert.equal(state.armed, true);
});

test('retry moves failure through reading to captured and preserves the saved point detail', () => {
  const failed = captureStateFromError(armed, { captureField: field, message: error });
  const reading = captureStateFromSnapshot(failed, {
    captureField: field, captureArmed: true, captureState: 'reading', captureError: null, message: '회차 버튼 · 예매창 화면 읽는 중',
  });
  assert.equal(reading.status, 'reading');
  assert.equal(reading.detail.includes('[target_not_foreground]'), false);
  const completed = { ...reading, status: 'captured', detail: '120, 300 저장됨.', armed: false };
  const state = captureStateFromSnapshot(completed, {
    captureField: field, captureArmed: false, captureState: 'captured', captureError: null,
  });
  assert.equal(state.status, 'captured');
  assert.equal(state.detail, '120, 300 저장됨.');
});

test('cancel and explicit rearm clear failure without erasing it on unrelated snapshots', () => {
  const failed = captureStateFromError(armed, { captureField: field, message: error });
  assert.equal(captureStateFromSnapshot(failed, { phase: 'idle' }).status, 'error');
  const cancelled = captureStateFromSnapshot(failed, { captureState: 'idle', captureArmed: false, captureError: null });
  assert.equal(cancelled.status, 'idle');
  assert.equal(cancelled.detail, '');
  const rearmed = captureStateFromSnapshot(failed, { captureState: 'armed', captureArmed: true, captureError: null });
  assert.equal(rearmed.status, 'armed');
  assert.equal(rearmed.detail, '');
});

test('unrelated runtime errors do not relabel the coordinate field as failed', () => {
  assert.equal(captureStateFromError(armed, { captureField: null, message: '서버 시간 측정 실패' }), armed);
});

test('failure snapshot and error event can arrive in either order', () => {
  const payload = { captureField: field, message: error };
  const snapshot = { captureField: field, captureArmed: true, captureState: 'error', captureError: error };
  const eventFirst = captureStateFromSnapshot(captureStateFromError(armed, payload), snapshot);
  const snapshotFirst = captureStateFromError(captureStateFromSnapshot(armed, snapshot), payload);
  assert.deepEqual(eventFirst, snapshotFirst);
});

test('recovery hints identify F7 reconnect for a changed popup and F8 retry for foreground failure', () => {
  assert.match(captureRecoveryHint('[target_window_changed] 창이 변경되었습니다.', true), /F7.*F8로 지정/);
  assert.match(captureRecoveryHint(error, true), /맨 앞으로.*F8/);
});

test('Accessibility recovery names the macro app and macOS setting without assuming Terminal permissions', () => {
  const hint = captureRecoveryHint('[accessibility_permission] 마우스 위치의 창을 확인할 권한이 없습니다.', true);
  assert.match(hint, /macOS 시스템 설정.*개인정보 보호 및 보안.*손쉬운 사용.*Integrated Macro.*허용/);
  assert.match(hint, /앱을 다시 실행.*F8로 지정/);
  assert.doesNotMatch(hint, /Terminal|터미널|화면 기록|screenPermission/);
});

test('unavailable hit test offers F8 retry and reconnect while ambiguous focus requires reconnect', () => {
  const hitTestHint = captureRecoveryHint('[target_hit_test_unavailable] 마우스 위치의 창을 확인하지 못했습니다.', true);
  assert.match(hitTestHint, /본문을 한 번 클릭.*F8.*계속 실패하면 F7/);
  const focusHint = captureRecoveryHint('[target_focus_unavailable] 입력 대상 창을 확인하지 못했습니다.', true);
  assert.match(focusHint, /본문을 클릭하고 F7.*F8로 지정/);
});

test('real occlusion keeps actionable obstruction removal and F8 retry', () => {
  assert.match(captureRecoveryHint('[target_occluded] 지정 위치가 다른 창에 가려져 있습니다.', true), /창이나 메뉴를 닫거나 옮기고.*F8/);
});

test('new native capture errors retain original details through snapshot updates', () => {
  for (const code of ['accessibility_permission', 'target_hit_test_unavailable', 'target_focus_unavailable', 'target_window_changed', 'target_occluded']) {
    const original = `[${code}] 예매창 확인 실패 · 창 ID 123013`;
    const failed = captureStateFromError(armed, { captureField: field, message: original });
    const state = captureStateFromSnapshot(failed, {
      captureField: field, captureArmed: true, captureState: 'error', captureError: original,
    });
    assert.equal(state.detail, original);
    assert.equal(state.status, 'error');
    assert.equal(state.armed, true);
  }
});

test('unknown capture failures retain armed versus disarmed retry instructions', () => {
  assert.match(captureRecoveryHint('[frame_timeout] 화면 읽기 시간 초과', true), /대상 위에서 F8을 다시/);
  assert.match(captureRecoveryHint('[frame_timeout] 화면 읽기 시간 초과', false), /이 항목의 F8로 지정을 다시/);
});
