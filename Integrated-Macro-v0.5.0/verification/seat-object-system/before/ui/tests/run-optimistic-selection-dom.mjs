import assert from 'node:assert/strict';
import { defaultProfile } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

const first = 'pixel:10:10';
const latest = 'pixel:20:20';
const queued = 'pixel:30:30';
const mode = process.argv[2] || 'seats';
assert.ok(['seats', 'normal', 'cancellation'].includes(mode));
const initial = {
  mode, selectionPolicy: 'optimistic', estimatedCount: 2, selectedCount: 1,
  selected_ids: [first], attempted_ids: [first, latest], current_candidate_id: queued,
  delivered_attempts: [{ action_id: 4, candidate_id: first, generation: 0 },
    { action_id: 5, candidate_id: latest, generation: 0 }],
  phase: 'seats', running: true, paused: false, runId: 1, revision: 1,
  captureState: 'idle', checks: [], manualActions: ['conflict'], targetCount: 2,
};
const profile = defaultProfile();
profile.desktop.selected_color = { r: 10, g: 20, b: 30 };
let main;
let overlay;
let revision = 1;
const emit = (change) => {
  const next = { ...initial, revision: ++revision, ...change };
  main.emit('state-change', next);
  overlay.emit('state-change', next);
  return next;
};
const invoke = async (name, args) => {
  assert.equal(name, 'confirm_observation');
  assert.equal(args.kind, 'conflict');
  assert.equal(args.runId, initial.runId);
  assert.equal(args.revision, revision, 'Conflict confirmation must use the current recovery snapshot revision');
  assert.equal(args.selectedIds, undefined, 'Runtime resolves latest delivered action, never the queued current candidate');
  return emit({
    estimatedCount: 1, attempted_ids: [first], current_candidate_id: null,
    delivered_attempts: initial.delivered_attempts.slice(0, 1), manualActions: [], targetRecovery: { active: false },
  });
};
main = await loadAppPage(new URL('../index.html', import.meta.url), initial, invoke, profile);
overlay = await loadAppPage(new URL('../overlay.html', import.meta.url), initial, invoke, profile);
const $ = (selector) => main.document.querySelector(selector);
const overlayCount = () => overlay.document.querySelector('#overlay-count').textContent;
assert.equal($('#selected-count').textContent, '2');
assert.equal($('#selected-count-label').textContent, '선택 추정');
assert.equal(overlayCount(), '선택 추정 2 / 2');
assert.equal($('#legacy-selection-color-controls').hidden, true);
assert.equal($('#optimistic-selection-help').hidden, false);
assert.match($('#manual-message').textContent, /선점 실패 안내는 직접 확인.*위치 20, 20.*나머지 선택 추정은 유지/);
assert.doesNotMatch($('#manual-message').textContent, /위치 30, 30/);
assert.equal($('#manual-actions').querySelector('select'), null);
let conflictButton = $('#manual-actions').querySelectorAll('button')
  .find((button) => button.textContent === '직전 클릭 선점 실패 확인');
assert.ok(conflictButton);
emit({
  phase: 'paused', paused: true, targetRecovery: { active: true, reason: '연결한 예매창 복귀 대기' },
});
assert.equal($('#phase-label').textContent, '예매창 복귀 대기 · 자동 재개');
assert.equal($('#selected-count').textContent, '2', 'Automatic recovery retains delivered estimates, excluding the queued third candidate');
assert.equal(overlayCount(), '선택 추정 2 / 2');
assert.equal($('#manual-confirmation').hidden, false);
assert.equal($('#manual-actions').querySelector('select'), null);
assert.match($('#manual-message').textContent, /위치 20, 20/);
assert.doesNotMatch($('#manual-message').textContent, /위치 30, 30/);
conflictButton = $('#manual-actions').querySelectorAll('button')
  .find((button) => button.textContent === '직전 클릭 선점 실패 확인');
assert.ok(conflictButton);
assert.equal(conflictButton.disabled, false, 'Recovery retains the explicit one-click failure action');
await conflictButton.click();
assert.equal($('#selected-count').textContent, '1');
assert.equal($('#selected-count-label').textContent, '선택 추정');
assert.equal(overlayCount(), '선택 추정 1 / 2');
emit({
  phase: 'paused', paused: true, targetRecovery: { active: false }, manualActions: [],
  estimatedCount: 1, attempted_ids: [first], current_candidate_id: null,
});
assert.equal($('#phase-label').textContent, '일시정지');
assert.equal($('#manual-confirmation').hidden, true, 'Deliberate pause does not offer recovery-only confirmation actions');
assert.equal($('#manual-actions').querySelectorAll('button').length, 0);
emit({ mode: 'normal', selectionPolicy: 'confirmed', estimatedCount: 2, current_candidate_id: null });
assert.equal($('#selected-count').textContent, '1');
assert.equal($('#selected-count-label').textContent, '확인된 선택');
assert.equal(overlayCount(), '1 / 2');
assert.equal($('#legacy-selection-color-controls').hidden, false);
assert.equal($('#optimistic-selection-help').hidden, true);
assert.ok($('#manual-actions').querySelector('select'), 'Legacy late-conflict confirmation still requires a specific seat');
console.log('PASS: Production optimistic selection preserves estimates and one-click conflict during automatic recovery, hides actions during manual pause, and keeps legacy controls.');
console.log('Not verified: native click delivery, site selection success, or automatic conflict detection.');
