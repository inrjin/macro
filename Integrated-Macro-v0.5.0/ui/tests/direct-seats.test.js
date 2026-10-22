import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { defaultProfile, normalizeSnapshot, targetRecoveryView, validateStart, visionOverlayLabel } from '../model.js';

test('direct seats require a valid common ROI even when a cancellation zone has one', () => {
  const profile = defaultProfile();
  profile.legacy.regions = [{ color: { r: 0, g: 0, b: 255 }, vision: { roi: { x: 0, y: 0, width: 10, height: 10 } } }];
  assert.throws(() => validateStart(profile, 'seats'), /좌석 영역.*첫 모서리.*반대 모서리/);
  for (const roi of [
    { x: -1, y: 0, width: 10, height: 10 },
    { x: 0, y: 0, width: 0, height: 10 },
    { x: 0, y: 0, width: 10, height: NaN },
  ]) {
    profile.vision.roi = roi;
    assert.throws(() => validateStart(profile, 'seats'), /좌석 바로 탐색/);
  }
  profile.vision.roi = { x: 10, y: 20, width: 30, height: 40 };
  assert.doesNotThrow(() => validateStart(profile, 'seats'));
});

test('direct seats ignore earlier booking settings while normal mode retains validation', () => {
  const profile = defaultProfile();
  profile.vision.roi = { x: 10, y: 20, width: 30, height: 40 };
  profile.desktop.scheduled_start = true;
  profile.features.auto_entry = true;
  profile.features.server_clock = true;
  assert.doesNotThrow(() => validateStart(profile, 'seats'));
  assert.throws(() => validateStart(profile, 'normal'), /판매 시작 시각/);
  assert.doesNotThrow(() => validateStart(profile, 'cancellation'));
  assert.equal(profile.desktop.scheduled_start, true, 'Bypassing old settings must not mutate them');
});

test('automatic target recovery is distinct from manual pause without rewriting engine phase', () => {
  const recovery = { phase: 'paused', paused: true, running: true, mode: 'seats',
    targetRecovery: { active: true, reason: '[target_not_foreground] 예매창이 전면이 아닙니다.' } };
  const snapshot = normalizeSnapshot(recovery);
  assert.equal(snapshot.phase, 'paused');
  assert.equal(snapshot.paused, true);
  assert.equal(snapshot.label, '예매창 복귀 대기 · 자동 재개');
  assert.match(snapshot.message, /target_not_foreground.*예매창을 다시 전면.*일시정지.*자동 재개를 취소/);
  assert.equal(targetRecoveryView({ ...recovery, targetRecovery: { active: false } }), null);
  const manual = normalizeSnapshot({ ...recovery, targetRecovery: { active: false } });
  assert.equal(manual.label, '일시정지');
  assert.equal(manual.paused, true);
  assert.equal(visionOverlayLabel({ phase: 'seats', targetRecovery: { active: true } }), '');
});

test('missing seat area is a displayed wait without changing run or selection state', () => {
  const message = '‘좌석 영역 · 첫 모서리’와 ‘좌석 영역 · 반대 모서리’를 F8로 지정하세요.';
  const state = normalizeSnapshot({ phase: 'seats', mode: 'normal', running: true, paused: false,
    waitingForSeatArea: true, phaseLabel: '좌석 탐색', message,
    selectionPolicy: 'optimistic', estimatedCount: 1, selectedCount: 0, targetCount: 2 });
  assert.equal(state.waitingForSeatArea, true);
  assert.equal(state.label, '좌석 영역 미지정 · 지정 대기');
  assert.equal(state.message, message);
  assert.equal(state.phase, 'seats');
  assert.equal(state.running, true, 'An active wait must still prevent duplicate starts');
  assert.equal(state.paused, false);
  assert.equal(state.displaySelectedCount, 1);
  assert.equal(state.selectedCount, 0);
  assert.equal(state.targetCount, 2);
  assert.equal(visionOverlayLabel(state), '');
  const resumed = normalizeSnapshot({ ...state, waitingForSeatArea: false });
  assert.equal(resumed.label, '좌석 탐색');
  assert.equal(resumed.running, true);
  assert.equal(normalizeSnapshot({ phase: 'seats' }).waitingForSeatArea, false);
  const paused = normalizeSnapshot({ phase: 'paused', paused: true, running: false, waitingForSeatArea: false });
  assert.equal(paused.label, '일시정지');
});

test('production recovery controls cancel automatic resume with pause and remain stoppable', () => {
  const result = spawnSync(process.execPath, [
    '--experimental-vm-modules', fileURLToPath(new URL('./run-recovery-dom.mjs', import.meta.url)),
  ], { encoding: 'utf8', timeout: 15000 });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
  assert.match(result.stdout, /PASS: Production target recovery controls/);
});
