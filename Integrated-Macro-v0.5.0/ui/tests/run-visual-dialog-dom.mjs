import assert from 'node:assert/strict';
import { defaultProfile } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

const profile = defaultProfile();
profile.features.conflict_recovery = true;
const mode = process.argv[2] || 'seats';
assert.ok(['seats', 'normal', 'cancellation'].includes(mode));
const initial = {
  phase: 'seats', mode, runId: 1, revision: 0, running: true, paused: false,
  captureState: 'idle', checks: [], manualActions: ['conflict'],
  selectionPolicy: 'optimistic', estimatedCount: 1, selectedCount: 0, targetCount: 2,
  attempted_ids: ['pixel:10:20'], delivered_attempts: [{ action_id: 1, candidate_id: 'pixel:10:20', generation: 0 }],
  message: '빈 좌석 탐색 중',
  visualDialog: { enabled: true, status: 'watching', changedPercent: null, message: '클릭 뒤 화면 변화 감시 중' },
  visionDiagnostics: {
    phase: 'seats', source: 'global', color: { r: 130, g: 70, b: 210 }, tolerance: 12,
    stats: { matching_pixels: 30, components_found: 3, accepted_count: 3, rejected_too_small: 0, rejected_too_large: 0 },
  },
};
const unexpectedInvoke = (name) => { throw new Error(`Unexpected native command: ${name}`); };
const main = await loadAppPage(new URL('../index.html', import.meta.url), initial, unexpectedInvoke, profile);
const overlay = await loadAppPage(new URL('../overlay.html', import.meta.url), initial, unexpectedInvoke, profile);
const $ = (selector) => main.document.querySelector(selector);
const overlayText = (id) => overlay.document.querySelector(`#overlay-${id}`).textContent;
let revision = 0;
const emit = (change) => {
  const value = { ...initial, revision: ++revision, ...change };
  main.emit('state-change', value);
  overlay.emit('state-change', value);
};
assert.equal(main.document.querySelectorAll('[data-setting="features.conflictRecovery"]').length, 1);
assert.equal($('[data-setting="features.conflictRecovery"]').checked, true);
assert.equal($('#conflict-recovery-label').textContent, '안내창 자동 대응');
assert.match($('#conflict-recovery-help').textContent, /중앙 화면 변화 또는 주변 어두워짐.*Esc.*부족분.*완료 버튼 클릭 직전/);
assert.equal($('#visual-dialog-status').textContent, '클릭 뒤 화면 변화 감시 중');
assert.equal($('#phase-message').textContent, '빈 좌석 탐색 중');
assert.equal($('#manual-kind').textContent, '필요할 때');
assert.doesNotMatch($('#manual-message').textContent, /F6/);
assert.equal($('#manual-actions').querySelectorAll('button').length, 1, 'Automatic recovery introduces no mandatory confirmation action');
assert.match(overlayText('phase'), /색상 후보 3개/);
emit({ phase: 'conflict', manualActions: [], visualDialog: {
  enabled: true, status: 'closing', changedPercent: 40.5, message: '화면 변화 감지 · Esc 전송 중',
} });
assert.equal($('#phase-label').textContent, '안내창 후보 감지 · Esc 처리 중');
assert.equal($('#phase-message').textContent, '화면 변화 감지 · Esc 전송 중');
assert.equal($('#visual-dialog-status').textContent, '화면 변화 감지 · Esc 전송 중 · 전체 화면 변화 40.5%');
assert.equal(overlayText('message'), '화면 변화 감지 · Esc 전송 중');
assert.doesNotMatch(overlayText('phase'), /색상 후보|선점 실패/);
assert.equal($('#manual-confirmation').hidden, true);
assert.equal($('#stop-button').disabled, false);
assert.equal($('#pause-button').disabled, false);
assert.equal($('#selected-count').textContent, '1', 'UI does not change estimates merely because pixels changed');
emit({ phase: 'conflict', manualActions: ['dialog_gone'], visualDialog: {
  enabled: true, status: 'closing', changedPercent: 40.5, message: '화면 변화 감지 · Esc 전송 중',
} });
assert.equal($('#manual-confirmation').hidden, false, 'Keep the manual fallback available');
assert.equal($('#manual-heading').textContent, '선택적 수동 확인');
assert.equal($('#manual-kind').textContent, '선택 사항');
assert.match($('#manual-message').textContent, /화면 복귀가 감지되면 별도 확인 없이 탐색/);
assert.doesNotMatch($('#manual-message').textContent, /F6|알려주세요/);
assert.equal($('#manual-actions').querySelectorAll('button').length, 1);
assert.equal($('#manual-actions').querySelector('button').textContent, '안내창 닫힘 직접 확인 · 선택 사항');
emit({ phase: 'conflict', manualActions: ['dialog_gone'], visualDialog: {
  enabled: true, status: 'waiting_restore', changedPercent: null, message: 'Esc 전송 완료 · 화면 복귀 확인 중',
} });
assert.equal($('#phase-label').textContent, '화면 복귀 확인 중');
assert.equal(overlayText('message'), 'Esc 전송 완료 · 화면 복귀 확인 중');
assert.equal($('#manual-heading').textContent, '선택적 수동 확인');
assert.doesNotMatch($('#manual-message').textContent, /F6|알려주세요/);
emit({ phase: 'completing', manualActions: ['completion', 'conflict'], message: '최종 완료 여부 확인 대기',
  visualDialog: { enabled: true, status: 'off', changedPercent: null, message: '선택 완료 단계 · 안내창 자동 대응 종료' } });
assert.equal($('#visual-dialog-status').hidden, true);
assert.equal($('#phase-label').textContent, '완료 버튼 처리');
assert.match($('#manual-message').textContent, /현재 감시 중이 아닙니다/);
assert.doesNotMatch($('#manual-message').textContent, /자동 대응을 사용 중/);
assert.equal($('#manual-actions').querySelectorAll('button').length, 1, 'Obsolete completion confirmation is removed; explicit failure fallback remains');
assert.equal($('#completion-help').hidden, false);
emit({ captureState: 'reading', captureField: 'color', captureArmed: true, message: 'F8 좌석 색상 읽는 중',
  visualDialog: { enabled: true, status: 'closing', changedPercent: 40, message: '가려져야 하는 이전 감지 상태' } });
assert.equal($('#visual-dialog-status').hidden, true);
assert.match(overlayText('phase'), /읽는 중/);
assert.equal(overlayText('message'), 'F8 좌석 색상 읽는 중');
emit({ phase: 'paused', paused: true, message: '직접 일시정지했습니다.', manualActions: [],
  visualDialog: { enabled: true, status: 'waiting_restore', changedPercent: null, message: '이전 화면 복귀 상태' } });
assert.equal($('#visual-dialog-status').hidden, true);
assert.equal($('#phase-label').textContent, '일시정지');
assert.equal(overlayText('message'), '직접 일시정지했습니다.');
emit({ mode: 'normal', selectionPolicy: 'confirmed', visualDialog: { enabled: false, status: 'off' } });
assert.equal($('#conflict-recovery-label').textContent, '선점 실패 시 다음 좌석 시도');
assert.match($('#conflict-recovery-help').textContent, /원본 선택 흐름.*직접 확인.*Esc/);
assert.equal($('#visual-dialog-status').hidden, true);
assert.equal($('#manual-kind').textContent, '직접 확인');
assert.equal(main.calls.filter(({ name }) => name !== 'get_bootstrap').length, 0);
console.log('PASS: Production visual dialog controls reuse the recovery toggle, preserve native messages and estimates, and keep capture/manual-pause priority.');
console.log('Not verified: image change classification, native Esc delivery, or real site dialog closure.');
