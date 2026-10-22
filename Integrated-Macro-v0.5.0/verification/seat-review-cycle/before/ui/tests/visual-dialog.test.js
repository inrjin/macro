import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { normalizeSnapshot, visionOverlayLabel, visualDialogView } from '../model.js';

const snapshot = (change = {}) => ({
  mode: 'seats', selectionPolicy: 'optimistic', phase: 'conflict', running: true, paused: false, captureState: 'idle',
  message: '기존 실행 상태',
  visualDialog: { enabled: true, status: 'closing', changedPercent: 42.25, message: '화면 변화 감지 · Esc 전송 중' },
  ...change,
});

test('visual dialog status names a candidate without confirming a seat failure', () => {
  const value = snapshot();
  const normalized = normalizeSnapshot(value);
  assert.equal(normalized.phase, 'conflict', 'Presentation must preserve the actual engine phase');
  assert.equal(normalized.label, '안내창 후보 감지 · Esc 처리 중');
  assert.equal(normalized.message, value.visualDialog.message);
  assert.equal(visualDialogView(value).detail, '화면 변화 감지 · Esc 전송 중 · 전체 화면 변화 42.3%');
  assert.doesNotMatch(normalized.label, /선점 실패/);
  assert.equal(visionOverlayLabel(snapshot({ phase: 'seats' })), '');
  const restored = normalizeSnapshot(snapshot({
    phase: 'seats', visualDialog: { enabled: true, status: 'watching', changedPercent: null, message: '클릭 뒤 화면 변화 감시 중' },
  }));
  assert.equal(restored.label, '좌석 탐색');
  assert.equal(restored.message, '기존 실행 상태', 'Watching must not replace ongoing scan progress');
  assert.equal(visualDialogView(restored).detail, '클릭 뒤 화면 변화 감시 중');
});

test('visual status cannot cover capture, pause, recovery, errors or observed selection', () => {
  for (const change of [
    { selectionPolicy: 'observed' }, { phase: 'paused', paused: true },
    { phase: 'faulted' }, { phase: 'completed' }, { targetRecovery: { active: true } },
    { captureError: '[target_occluded] blocked' },
    ...['arming', 'armed', 'reading', 'error', 'captured'].map((captureState) => ({ captureState })),
    { visualDialog: { enabled: false, status: 'closing' } },
    { visualDialog: { enabled: true, status: 'off' } },
    { visualDialog: null },
  ]) assert.equal(visualDialogView(snapshot(change)), null);
  const paused = normalizeSnapshot(snapshot({ phase: 'paused', paused: true }));
  assert.equal(paused.label, '일시정지');
  const recovering = normalizeSnapshot(snapshot({ phase: 'paused', paused: true, targetRecovery: { active: true } }));
  assert.equal(recovering.label, '예매창 복귀 대기 · 자동 재개');
});

for (const mode of ['seats', 'normal', 'cancellation']) {
  test(`production ${mode} optimistic flow retains optional dialog fallback and ends monitoring honestly`, () => {
    const result = spawnSync(process.execPath, [
      '--experimental-vm-modules', fileURLToPath(new URL('./run-visual-dialog-dom.mjs', import.meta.url)), mode,
    ], { encoding: 'utf8', timeout: 15000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
    assert.match(result.stdout, /PASS: Production visual dialog controls/);
  });
}
