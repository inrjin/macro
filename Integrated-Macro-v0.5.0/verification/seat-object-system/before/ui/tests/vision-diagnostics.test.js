import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { normalizeSnapshot, visionDiagnosticsView, visionOverlayLabel } from '../model.js';

const diagnostic = (stats = {}) => ({
  phase: 'seats',
  source: 'global',
  color: { r: 130, g: 70, b: 210 },
  tolerance: 12,
  roi: { x: 20, y: 30, width: 400, height: 300 },
  frame: { width: 1600, height: 1000, scaleX: 2, scaleY: 2 },
  stats: { matching_pixels: 0, components_found: 0, accepted_count: 0,
    rejected_too_small: 0, rejected_too_large: 0, ...stats },
});

test('unscanned or incomplete diagnostics never fabricate zero candidates', () => {
  for (const value of [undefined, null, {}, { phase: 'seats' }, { ...diagnostic(), stats: {} }]) {
    assert.equal(visionDiagnosticsView(value), null);
    assert.equal(visionOverlayLabel({ phase: 'seats', visionDiagnostics: value }), '');
  }
});

test('zero matching pixels and rejected components explain different causes', () => {
  const empty = visionDiagnosticsView(diagnostic());
  assert.equal(empty.candidates, '색상 후보 0개');
  assert.match(empty.reason, /일치 픽셀 0개.*색상과 탐색 범위/);
  assert.doesNotMatch(empty.reason, /크기 조건/);
  const rejected = visionDiagnosticsView(diagnostic({
    matching_pixels: 700, components_found: 4, rejected_too_small: 3, rejected_too_large: 1,
  }));
  assert.match(rejected.reason, /일치 픽셀 700개.*후보 크기 조건.*너무 작음 3개.*너무 큼 1개/);
  assert.doesNotMatch(rejected.reason, /픽셀이 없습니다/);
});

test('diagnostics show actual effective color and active source without promising seats', () => {
  const data = diagnostic({ matching_pixels: 24, components_found: 2, accepted_count: 2 });
  data.source = 'legacy_region_1';
  data.color = { r: 1, g: 128, b: 255 };
  const view = visionDiagnosticsView(data);
  assert.equal(view.candidates, '색상 후보 2개');
  assert.match(view.settings, /취소표 2구역 탐색.*#0180FF.*허용 오차 12/);
  assert.match(view.geometry, /20, 30.*400 × 300.*1600 × 1000.*배율 2 × 2/);
  assert.doesNotMatch(view.candidates, /좌석|선택 완료/);
});

test('seat fragments rejected for lack of clickable area explain the zero-candidate result', () => {
  const view = visionDiagnosticsView(diagnostic({
    matching_pixels: 3, components_found: 1, accepted_count: 0, rejected_unsupported: 1,
  }));
  assert.equal(view.candidates, '색상 후보 0개');
  assert.match(view.reason, /클릭할 면적이 부족한 색상 조각 1개.*좌석 내부의 색/);
});

test('pre-refresh image statistics never appear as actionable overlay candidates', () => {
  const data = { ...diagnostic({ matching_pixels: 24, components_found: 2, accepted_count: 2 }),
    awaitingRefreshImage: true };
  const view = visionDiagnosticsView(data);
  assert.equal(view.heading, '새로고침 전 화면 · 새 화면 대기');
  assert.equal(view.candidates, '이전 화면 참고값');
  assert.match(view.reason, /새로고침 전 화면의 참고 수치.*일치 픽셀 24개.*크기 조건 통과 2개.*새 화면/);
  assert.equal(visionOverlayLabel({ phase: 'seats', visionDiagnostics: data }), '');
  data.awaitingRefreshImage = false;
  assert.equal(visionOverlayLabel({ phase: 'seats', visionDiagnostics: data }), '색상 후보 2개');
});

test('last scan never replaces paused or waiting-region phase', () => {
  for (const phase of ['paused', 'waiting_region', 'region']) {
    const snapshot = normalizeSnapshot({ phase, visionDiagnostics: diagnostic() });
    const label = snapshot.label;
    assert.equal(visionDiagnosticsView(snapshot.visionDiagnostics).heading, '최근 좌석 탐색');
    assert.equal(visionOverlayLabel(snapshot), '');
    assert.equal(snapshot.phase, phase);
    assert.equal(snapshot.label, label);
  }
  const region = { ...diagnostic(), phase: 'region', source: 'region_map' };
  assert.equal(visionDiagnosticsView(region).heading, '최근 구역 탐색');
  assert.match(visionDiagnosticsView(region).settings, /구역 선택 설정/);
  assert.equal(visionOverlayLabel({ phase: 'seats', visionDiagnostics: region }), '');
});

test('overlay candidate count appears only during active seat scans outside F8/errors', () => {
  const snapshot = { phase: 'seats', visionDiagnostics: diagnostic(), captureState: 'idle' };
  assert.equal(visionOverlayLabel(snapshot), '색상 후보 0개');
  for (const captureState of ['arming', 'armed', 'reading', 'error', 'captured']) {
    assert.equal(visionOverlayLabel({ ...snapshot, captureState }), '');
  }
  assert.equal(visionOverlayLabel({ ...snapshot, captureError: '[target_occluded] blocked' }), '');
  assert.equal(visionOverlayLabel({ ...snapshot, phase: 'faulted' }), '');
});

test('actual settings and overlay listeners render scan diagnostics without changing phase', () => {
  const result = spawnSync(process.execPath, [
    '--experimental-vm-modules', fileURLToPath(new URL('./run-vision-dom.mjs', import.meta.url)),
  ], { encoding: 'utf8', timeout: 15000 });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
  assert.match(result.stdout, /PASS: Production vision diagnostics/);
});
