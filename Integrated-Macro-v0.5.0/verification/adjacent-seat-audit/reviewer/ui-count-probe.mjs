import assert from 'node:assert/strict';
import { defaultProfile } from '../../../ui/model.js';
import { loadAppPage } from '../../../ui/tests/dom-fixture.mjs';

// Actual index.html and app/model listeners, with a stubbed IPC boundary.
// This does not execute the Engine, browser layout, Tauri, or native input.
const rows = [];
for (const mode of ['normal', 'cancellation', 'seats']) {
  for (const trigger of ['button', 'shortcut']) {
    const payloads = [];
    for (const targetCount of [2, 3]) {
      const profile = defaultProfile();
      profile.vision.roi = { x: 10, y: 20, width: 100, height: 80 };
      const initial = { phase: 'idle', mode: 'normal', running: false, paused: false,
        runId: 0, revision: 0, captureState: 'idle', message: '', checks: [],
        selectedCount: 0, targetCount: 1, manualActions: [] };
      let sent;
      const page = await loadAppPage(new URL('../../../ui/index.html', import.meta.url), initial,
        async (name, args) => {
          assert.equal(name, trigger === 'button' ? 'start_run' : 'start_shortcut');
          assert.equal(args.error ?? null, null);
          sent = structuredClone(args);
          return { profile: args.profile, profileRevision: 1, snapshot: {
            ...initial, mode, phase: 'seats', running: true, runId: 1, revision: 1,
            targetCount: args.profile.target_seats,
          } };
        }, profile);
      const count = page.document.querySelector('[data-setting="targetCount"]');
      assert.deepEqual(count.options.map((option) => option.textContent), ['1석', '2석', '3석', '4석']);
      count.value = String(targetCount);
      for (const listener of count.listeners.input || []) listener({ target: count });
      for (const radio of page.document.querySelectorAll('input[name="run-mode"]')) {
        radio.checked = radio.value === mode;
      }
      const selectedMode = page.document.querySelector('input[name="run-mode"]:checked');
      for (const listener of selectedMode.listeners.change || []) listener();
      if (trigger === 'button') await page.document.querySelector('#start-button').click();
      else await page.listeners['shortcut-start-requested'][0]({ payload: {
        requestId: 1, mode, shortcut: { normal: 'F1', cancellation: 'F2', seats: 'F5' }[mode],
      } });
      assert.ok(sent?.profile, `${mode}/${trigger}/${targetCount} must send a profile`);
      assert.equal(sent.profile.target_seats, targetCount);
      assert.equal(Number(page.document.querySelector('#target-count').textContent), targetCount);
      rows.push({ mode, trigger, visibleChoice: `${targetCount}석`, sentTargetSeats: sent.profile.target_seats,
        selectionPolicy: 'No adjacency setting is present in the emitted default-profile contract' });
      const { target_seats, ...withoutCount } = sent.profile;
      payloads.push(withoutCount);
    }
    assert.deepEqual(payloads[0], payloads[1], 'Changing 2석 to 3석 changes only target_seats');
  }
}
console.log(JSON.stringify({ scope: 'Actual UI listeners; stubbed IPC; no native input', cases: rows.length, rows }, null, 2));
