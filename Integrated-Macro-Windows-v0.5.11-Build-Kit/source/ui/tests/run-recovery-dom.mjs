import assert from 'node:assert/strict';
import { loadAppPage } from './dom-fixture.mjs';

const initial = {
  phase: 'paused', paused: true, running: true, mode: 'seats', runId: 1, revision: 1,
  targetRecovery: { active: true, reason: '연결한 예매창이 가려져 있습니다.' },
  captureState: 'idle', checks: [], manualActions: [], selectedCount: 0, targetCount: 2,
};
let main;
let overlay;
let revision = 1;
const emit = (changes) => {
  const next = { ...initial, revision: ++revision, ...changes };
  main.emit('state-change', next);
  overlay.emit('state-change', next);
  return next;
};
const invoke = async (name) => {
  if (name === 'pause_run') {
    return emit({ targetRecovery: { active: false }, message: '사용자가 일시정지했습니다.' });
  }
  assert.equal(name, 'stop_run');
  return emit({ phase: 'stopped', paused: false, running: false, targetRecovery: { active: false } });
};
main = await loadAppPage(new URL('../index.html', import.meta.url), initial, invoke);
overlay = await loadAppPage(new URL('../overlay.html', import.meta.url), initial, invoke);
const $ = (selector) => main.document.querySelector(selector);
assert.equal($('#phase-label').textContent, '예매창 복귀 대기 · 자동 재개');
assert.equal(overlay.document.querySelector('#overlay-phase').textContent, '예매창 복귀 대기 · 자동 재개');
assert.match($('#phase-message').textContent, /가려져.*예매창을 다시 전면.*일시정지/);
assert.equal($('#start-button').disabled, false, 'Recovery permits a replacement execution');
assert.equal($('#pause-button').disabled, false);
assert.equal($('#stop-button').disabled, false);
assert.match($('#pause-button').textContent, /일시정지/);
assert.doesNotMatch($('#pause-button').textContent, /다시 진행/);
await $('#pause-button').click();
assert.equal(main.calls.at(-1).name, 'pause_run', 'Recovery is cancelled by pause, not forcibly resumed');
assert.equal($('#phase-label').textContent, '일시정지');
assert.match($('#pause-button').textContent, /다시 진행/);
emit({ targetRecovery: initial.targetRecovery });
await $('#stop-button').click();
assert.equal(main.calls.at(-1).name, 'stop_run');
assert.equal($('#phase-label').textContent, '중지됨');
assert.equal($('#start-button').disabled, false);
console.log('PASS: Production target recovery controls preserve manual pause and allow stop.');
console.log('Not verified: native target recovery, browser focus, or desktop input.');
