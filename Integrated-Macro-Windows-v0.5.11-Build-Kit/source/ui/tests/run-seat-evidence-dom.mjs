import assert from 'node:assert/strict';
import { defaultProfile } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

for (const mode of ['normal', 'seats', 'cancellation']) {
  const profile = defaultProfile();
  profile.desktop.selected_color = { r: 10, g: 20, b: 30 };
  const initial = { phase: 'idle', mode, running: false, paused: false, runId: 0, revision: 0,
    selectionCountVerification: false, captureState: 'idle', checks: [], manualActions: [] };
  const confirmations = [];
  let latestSnapshot = initial;
  const page = await loadAppPage(new URL('../index.html', import.meta.url), initial, async (name, args) => {
    assert.equal(name, 'confirm_observation');
    confirmations.push(args);
    return { ...latestSnapshot, revision: latestSnapshot.revision + 1, manualActions: [] };
  }, profile);
  const overlay = await loadAppPage(new URL('../overlay.html', import.meta.url), initial, undefined, profile);
  const $ = selector => page.document.querySelector(selector);
  const change = (setting, value) => {
    for (const el of page.document.querySelectorAll(`[data-setting="${setting}"]`)) {
      if (el.type === 'checkbox') el.checked = value; else el.value = String(value);
      for (const cb of el.listeners.input || []) cb({ target: el });
    }
  };
  change('refreshMode', 'legacyKeys');
  for (const radio of page.document.querySelectorAll('input[name="run-mode"]')) radio.checked = radio.value === mode;
  for (const cb of $('input[name="run-mode"]:checked').listeners.change || []) cb();
  const check = (count, optimistic) => {
    assert.equal($('#count-selection-help').hidden, !count, `${mode}: count-specific policy`);
    assert.equal($('#legacy-selection-color-controls').hidden, count || optimistic);
    assert.equal($('#optimistic-selection-help').hidden, !optimistic || count);
    if (count) {
      assert.match($('#count-selection-help').textContent, /사이트의 선택 좌석 수.*읽지 못하거나 예상과 다르면/);
      assert.doesNotMatch($('#count-selection-help').textContent, /선택 추정/);
    }
    assert.equal($('#conflict-recovery-label').textContent, count || optimistic ? '안내창 자동 대응' : '선점 실패 시 다음 좌석 시도');
    assert.match($('#conflict-recovery-help').textContent, count || optimistic ? /중앙 화면 변화.*Esc/ : /직접 확인해 알리면/);
  };
  const optimistic = mode === 'seats';
  check(false, optimistic);
  change('features.nolSelectionCount', true);
  check(true, optimistic); // Unsaved draft overrides the idle snapshot.
  change('features.nolSelectionCount', false);
  check(false, optimistic); // OFF restores the original guidance.
  let revision = 0;
  const emit = change => {
    const payload = { ...initial, runId: 1, revision: ++revision,
      phase: 'seats', running: true, selectionPolicy: optimistic ? 'optimistic' : 'observed',
      siteSelectionCount: 1, selectionCountPending: false, targetCount: 2, ...change };
    latestSnapshot = payload;
    page.emit('state-change', payload);
    overlay.emit('state-change', payload);
  };
  emit({ selectionCountVerification: true });
  check(true, optimistic); // Active snapshot overrides the unchecked draft.
  assert.equal(overlay.document.querySelector('#overlay-count').textContent, '사이트 선택 수 1 / 2');
  emit({ phase: 'paused', running: false, paused: true, selectionCountVerification: true });
  check(true, optimistic);
  change('features.nolSelectionCount', true);
  emit({ selectionCountVerification: false });
  check(false, optimistic); // Checked draft cannot change a running policy.
  const delivered = [{ action_id: 4, candidate_id: 'pixel:20:20', generation: 0 }];
  const failure = { selectionCountVerification: true, manualActions: ['selection', 'conflict'],
    current_candidate_id: 'pixel:30:30', delivered_attempts: delivered,
    visualDialog: { enabled: true, status: 'watching' } };
  emit(failure);
  assert.equal($('#manual-heading').textContent, '직접 확인한 실패 알리기');
  assert.equal($('#manual-kind').textContent, '필요할 때');
  assert.equal($('#manual-actions').querySelector('select'), null);
  assert.equal($('#manual-actions').querySelector('.selection-confirmation'), null);
  assert.match($('#manual-message').textContent, /위치 20, 20.*사이트의 선택 좌석 수를 다시 확인/);
  assert.match($('#manual-message').textContent, /안내창 자동 대응을 사용 중/);
  assert.doesNotMatch($('#manual-message').textContent, /위치 30, 30|선택 추정을 제외|현재 감시 중이 아닙니다/);
  emit({ ...failure, phase: 'conflict', manualActions: ['dialog_gone'],
    visualDialog: { enabled: true, status: 'waiting_restore' } });
  assert.equal($('#manual-heading').textContent, '선택적 수동 확인');
  assert.equal($('#manual-kind').textContent, '선택 사항');
  assert.match($('#manual-actions').querySelector('button').textContent, /선택 사항/);
  emit({ ...failure, phase: 'paused', running: false, paused: true,
    targetRecovery: { active: true, reason: '예매창 복귀 대기' } });
  const button = $('#manual-actions').querySelector('button');
  assert.equal(button.textContent, '직전 클릭 선점 실패 확인');
  assert.equal(button.disabled, false);
  await button.click();
  assert.equal(confirmations.length, 1);
  assert.equal(confirmations[0].kind, 'conflict');
  assert.equal(confirmations[0].selectedIds, undefined, 'runtime selects the latest delivered attempt, not the queued candidate');
  assert.equal(confirmations[0].runId, 1);
}
console.log('PASS: Production count guidance follows unsaved idle settings and authoritative running/paused snapshots across all modes; OFF restores original controls.');
console.log('Not verified: browser layout, native input, text recognition or site selection state.');
