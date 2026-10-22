import assert from 'node:assert/strict';
import { defaultProfile, mergeProfile, toForm, fromForm } from '../../../ui/model.js';
import { loadAppPage } from '../../../ui/tests/dom-fixture.mjs';

assert.equal(defaultProfile().features.adjacent_seats, false);
assert.equal(mergeProfile({ features: { seat_priority: true } }).features.adjacent_seats, false);
for (const enabled of [false, true]) {
  const imported = mergeProfile({ target_seats: 3, features: { adjacent_seats: enabled } });
  assert.equal(toForm(imported).features.adjacentSeats, enabled);
  assert.equal(fromForm(toForm(imported), imported).features.adjacent_seats, enabled);
}

const rows = [];
for (const mode of ['normal', 'cancellation', 'seats']) {
  for (const trigger of ['button', 'shortcut']) {
    for (const enabled of [false, true]) {
      for (const count of [2, 3]) {
        const profile = defaultProfile();
        profile.vision.roi = { x: 10, y: 20, width: 100, height: 80 };
        // A persisted opposite option ensures the current form actually wins.
        profile.features.adjacent_seats = !enabled;
        const initial = { phase: 'idle', mode: 'normal', running: false, paused: false,
          runId: 0, revision: 0, captureState: 'idle', message: '', checks: [],
          selectedCount: 0, targetCount: 2, manualActions: [] };
        let sent;
        const page = await loadAppPage(new URL('../../../ui/index.html', import.meta.url), initial,
          async (name, args) => {
            assert.equal(name, trigger === 'button' ? 'start_run' : 'start_shortcut');
            assert.equal(args.error ?? null, null);
            sent = structuredClone(args.profile);
            return { profile: args.profile, profileRevision: 1, snapshot: {
              ...initial, mode, phase: 'seats', running: true, runId: 1, revision: 1,
              targetCount: args.profile.target_seats,
            } };
          }, profile);
        const input = (setting, value) => {
          const controls = page.document.querySelectorAll(`[data-setting="${setting}"]`);
          assert.ok(controls.length > 0, `actual UI is missing ${setting}`);
          const control = controls[0];
          if (control.type === 'checkbox') control.checked = value;
          else control.value = String(value);
          for (const listener of control.listeners.input || []) listener({ target: control });
        };
        assert.match(page.document.querySelector('#feature-list').textContent, /연석 우선/);
        input('features.adjacentSeats', enabled);
        input('targetCount', count);
        for (const radio of page.document.querySelectorAll('input[name="run-mode"]')) {
          radio.checked = radio.value === (trigger === 'shortcut' && mode !== 'normal' ? 'normal' : mode);
        }
        for (const listener of page.document.querySelector('input[name="run-mode"]:checked').listeners.change || []) listener();
        if (trigger === 'button') await page.document.querySelector('#start-button').click();
        else await page.listeners['shortcut-start-requested'][0]({ payload: {
          requestId: 1, mode, shortcut: { normal: 'F1', cancellation: 'F2', seats: 'F5' }[mode],
        } });
        assert.equal(sent?.features.adjacent_seats, enabled);
        assert.equal(sent?.target_seats, count);
        rows.push({ mode, trigger, requestedCount: count, currentOption: enabled,
          persistedOppositeOption: !enabled, sentOption: sent.features.adjacent_seats });
      }
    }
  }
}
console.log(JSON.stringify({ scope: 'Actual HTML/app/model listeners with stubbed IPC; no Engine or OS execution',
  defaultOff: true, importAndFormRoundTrip: true, cases: rows.length, rows }, null, 2));
