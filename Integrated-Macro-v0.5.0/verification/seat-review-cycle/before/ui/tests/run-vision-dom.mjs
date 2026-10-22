import assert from 'node:assert/strict';
import { loadAppPage } from './dom-fixture.mjs';

const initial = { phase: 'waiting_region', runId: 1, revision: 0, captureState: 'idle',
  message: '구역 화면 표시를 기다립니다.', checks: [], manualActions: [], selectedCount: 0, targetCount: 2 };
const main = await loadAppPage(new URL('../index.html', import.meta.url), initial);
const overlay = await loadAppPage(new URL('../overlay.html', import.meta.url), initial);
const $ = (selector) => main.document.querySelector(selector);
const overlayPhase = () => overlay.document.querySelector('#overlay-phase').textContent;
let revision = 0;
const emit = (state) => {
  const payload = { ...initial, revision: ++revision, ...state };
  main.emit('state-change', payload);
  overlay.emit('state-change', payload);
};
assert.equal($('#vision-diagnostics').hidden, true);
assert.equal($('#vision-candidates').textContent, '');
assert.equal($('#phase-label').textContent, '구역 화면 표시 대기');
assert.doesNotMatch(overlayPhase(), /후보/);
const diagnostic = {
  phase: 'seats', source: 'legacy_region_0', color: { r: 12, g: 34, b: 56 }, tolerance: 12,
  roi: { x: 30, y: 40, width: 500, height: 400 },
  frame: { width: 2000, height: 1500, scaleX: 2, scaleY: 2 },
  stats: { matching_pixels: 0, components_found: 0, accepted_count: 0, rejected_too_small: 0, rejected_too_large: 0 },
};
emit({ phase: 'seats', visionDiagnostics: diagnostic });
assert.equal($('#vision-diagnostics').hidden, false);
assert.equal($('#vision-heading').textContent, '최근 좌석 탐색');
assert.equal($('#vision-candidates').textContent, '색상 후보 0개');
assert.match($('#vision-settings').textContent, /취소표 1구역.*#0C2238/);
assert.match($('#vision-reason').textContent, /일치 픽셀 0개/);
assert.match(overlayPhase(), /좌석 탐색 · 색상 후보 0개/);
const beforeRefresh = { ...diagnostic, awaitingRefreshImage: true,
  stats: { ...diagnostic.stats, matching_pixels: 30, components_found: 3, accepted_count: 3 } };
emit({ phase: 'seats', visionDiagnostics: beforeRefresh });
assert.equal($('#vision-heading').textContent, '새로고침 전 화면 · 새 화면 대기');
assert.equal($('#vision-candidates').textContent, '이전 화면 참고값');
assert.match($('#vision-reason').textContent, /새로고침 전 화면의 참고 수치.*크기 조건 통과 3개/);
assert.doesNotMatch(overlayPhase(), /후보/);
emit({ phase: 'seats', visionDiagnostics: { ...beforeRefresh, awaitingRefreshImage: false } });
assert.equal($('#vision-candidates').textContent, '색상 후보 3개');
assert.match(overlayPhase(), /색상 후보 3개/);
const rejected = { ...diagnostic, stats: { matching_pixels: 40, components_found: 5, accepted_count: 0,
  rejected_too_small: 4, rejected_too_large: 1 } };
emit({ phase: 'seats', visionDiagnostics: rejected });
assert.match($('#vision-reason').textContent, /크기 조건.*너무 작음 4개.*너무 큼 1개/);
emit({ phase: 'paused', paused: true, visionDiagnostics: rejected });
assert.equal($('#phase-label').textContent, '일시정지');
assert.equal($('#vision-heading').textContent, '최근 좌석 탐색');
assert.doesNotMatch(overlayPhase(), /후보/);
const region = { ...diagnostic, phase: 'region', source: 'region_map' };
emit({ phase: 'waiting_region', visionDiagnostics: region });
assert.equal($('#phase-label').textContent, '구역 화면 표시 대기');
assert.equal($('#vision-heading').textContent, '최근 구역 탐색');
assert.doesNotMatch(overlayPhase(), /후보/);
emit({ phase: 'seats', visionDiagnostics: diagnostic, captureState: 'reading',
  captureField: 'color', captureArmed: true, message: 'F8 좌석 색상 읽는 중' });
assert.match(overlayPhase(), /읽는 중/);
assert.doesNotMatch(overlayPhase(), /후보/);
emit({ phase: 'seats', visionDiagnostics: diagnostic, captureState: 'error',
  captureField: 'color', captureArmed: true, captureError: '[target_occluded] 가려짐' });
assert.match(overlayPhase(), /위치 지정 실패/);
assert.doesNotMatch(overlayPhase(), /후보/);
emit({ phase: 'idle', runId: 2, visionDiagnostics: null });
assert.equal($('#vision-diagnostics').hidden, true);
assert.equal($('#vision-candidates').textContent, '');
console.log('PASS: Production vision diagnostics preserve phase, distinguish zero match/size rejection, and suppress overlay counts during F8 or absent scans.');
console.log('Not verified: real browser rendering, native capture results or desktop input.');
