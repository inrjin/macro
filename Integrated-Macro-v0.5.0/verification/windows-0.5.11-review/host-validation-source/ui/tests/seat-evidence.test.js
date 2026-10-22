import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { defaultProfile, mergeProfile, toForm, fromForm, normalizeSnapshot, visionDiagnosticsView, visualDialogView } from '../model.js';

test('new seat evidence options preserve old profiles and round-trip independently', () => {
  const profile = mergeProfile({ schema_version: 1, features: { adjacent_seats: true } });
  assert.equal(profile.features.seat_objects, false);
  assert.equal(profile.features.nol_selection_count, false);
  profile.features.nol_selection_count = true;
  const form = toForm(profile);
  assert.equal(form.features.nolSelectionCount, true);
  assert.equal(form.features.seatObjects, false);
  const restored = fromForm(form, profile);
  assert.equal(restored.features.nol_selection_count, true);
  assert.equal(restored.features.seat_objects, false);
  assert.equal(defaultProfile().features.nol_selection_count, false);
});

test('unreadable site count is not presented as zero or the click estimate', () => {
  const snapshot = normalizeSnapshot({ mode: 'seats', selectionPolicy: 'optimistic',
    selectionCountVerification: true, siteSelectionCount: null, selectionCountPending: true,
    estimatedCount: 2, targetCount: 2 });
  assert.equal(snapshot.displaySelectedCount, '—');
  assert.equal(snapshot.selectionCountLabel, '사이트 수량 확인 중');
});

test('site count stays distinct from delivered and queued clicks', () => {
  for (const mode of ['normal', 'seats', 'cancellation']) {
    const snapshot = normalizeSnapshot({ mode, selectionPolicy: 'optimistic',
      selectionCountVerification: true, siteSelectionCount: 1, selectionCountPending: false,
      estimatedCount: 2, selectedCount: 0, targetCount: 2 });
    assert.equal(snapshot.displaySelectedCount, 1);
    assert.equal(snapshot.selectionCountLabel, '사이트 선택 수');
    assert.equal(snapshot.estimatedCount, 2);
  }
});

test('object diagnostics distinguish one bridged color component from two seat objects', () => {
  const diagnostic = { phase: 'seats', color: { r: 190, g: 220, b: 180 },
    stats: { matching_pixels: 80, components_found: 1, accepted_count: 2, rejected_too_small: 0, rejected_too_large: 0 },
    objectStats: { resolved_objects: 2, ambiguous_objects: 0, reference_objects: 6 } };
  const view = visionDiagnosticsView(diagnostic);
  assert.equal(view.candidates, '좌석 객체 후보 2개');
  assert.match(view.reason, /색상 성분 1개에서 좌석 객체 후보 2개/);
  assert.doesNotMatch(view.reason, /중 2개|크기 조건/);
  const stale = visionDiagnosticsView({ ...diagnostic, awaitingRefreshImage: true });
  assert.equal(stale.candidates, '이전 화면 참고값');
  assert.match(stale.reason, /새로고침 전 화면/);
  assert.doesNotMatch(stale.reason, /크기 조건/);
});

test('ambiguous object diagnostics explain evidence rather than a fabricated size rejection', () => {
  const view = visionDiagnosticsView({ phase: 'seats', color: { r: 80, g: 160, b: 50 },
    stats: { matching_pixels: 80, components_found: 2, accepted_count: 0, rejected_too_small: 0, rejected_too_large: 0 },
    objectStats: { resolved_objects: 0, ambiguous_objects: 2, reference_objects: 6 },
    objectReasons: { outline_only_color: 2 } });
  assert.match(view.reason, /외곽선 색만 일치 2개/);
  assert.match(view.reason, /계속 탐색/);
  assert.doesNotMatch(view.reason, /후보 크기 조건/);
});

test('production policy help uses count verification in draft and active modes', () => {
  const result = spawnSync(process.execPath, [
    '--experimental-vm-modules', fileURLToPath(new URL('./run-seat-evidence-dom.mjs', import.meta.url)),
  ], { encoding: 'utf8', timeout: 15000 });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
  assert.match(result.stdout, /PASS: Production count guidance/);
});

test('count verification shows actual visual monitoring for legacy policies without masking holds', () => {
  const state = { phase: 'seats', selectionPolicy: 'observed', selectionCountVerification: true,
    visualDialog: { enabled: true, status: 'watching' } };
  assert.equal(visualDialogView(state).status, 'watching');
  assert.equal(visualDialogView({ ...state, selectionCountVerification: false }), null);
  for (const hold of [{ paused: true }, { targetRecovery: { active: true } }, { captureState: 'reading' }]) {
    assert.equal(visualDialogView({ ...state, ...hold }), null);
  }
});
