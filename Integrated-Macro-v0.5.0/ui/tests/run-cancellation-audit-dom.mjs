import assert from 'node:assert/strict';
import { applyCapture, defaultProfile, fromForm, toForm } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';
const html = new URL('../index.html', import.meta.url);
const clone = value => JSON.parse(JSON.stringify(value));
const q = (page, selector) => page.document.querySelector(selector);
const initial = { phase: 'idle', runId: 0, revision: 0, mode: 'cancellation', running: false, paused: false, captureState: 'idle', checks: [], manualActions: [] };
const frame = { width: 1200, height: 800, scale_x: 1, scale_y: 1 };
function change(page, setting, value) {
  for (const el of page.document.querySelectorAll(`[data-setting="${setting}"]`)) {
    if (el.type === 'checkbox') el.checked = value; else el.value = String(value);
    for (const cb of el.listeners.input || []) cb({ target: el });
  }
}
function selectCancellation(page) {
  for (const radio of page.document.querySelectorAll('input[name="run-mode"]')) radio.checked = radio.value === 'cancellation';
  for (const cb of q(page, 'input[name="run-mode"]:checked').listeners.change || []) cb();
}
async function start(page, trigger) {
  if (trigger === 'button') await q(page, '#start-button').click();
  else await page.listeners['shortcut-start-requested'][0]({ payload: { requestId: 1, mode: 'cancellation' } });
}

// Compare the actual two UI ingress paths for each supported refresh/navigation
// combination. Execution-only masking belongs to runtime; saved options remain.
for (const mode of ['automatic', 'internalButton', 'legacyRegions', 'legacyKeys']) {
  for (const refresh of [false, true]) {
    for (const hasTarget of [false, true]) {
      const payloads = [];
      for (const trigger of ['button', 'F2']) {
        const base = defaultProfile();
        base.features.auto_entry = true; base.features.region_priority = true; base.features.server_clock = true;
        base.desktop.scheduled_start = true;
        base.desktop.refresh_mode = 'button'; base.features.automatic_refresh = true;
        base.desktop.capture_points = { legacy_region_first: { x: 10, y: 20 }, legacy_region_second: { x: 30, y: 40 } };
        base.vision.roi = { x: 100, y: 120, width: 500, height: 400 };
        base.legacy.complete = { type: 'point', point: { x: 900, y: 700 } };
        base.legacy.regions = [{ color: { r: 12, g: 34, b: 56 }, vision: null, complete: null }];
        if (hasTarget) base.refresh_target = { type: 'point', point: { x: 1000, y: 100 } };
        const page = await loadAppPage(html, initial, async (name, args) => {
          assert.equal(name, trigger === 'button' ? 'start_run' : 'start_shortcut');
          assert.ok(args.profile);
          payloads.push(clone(args.profile));
          return { profile: args.profile, profileRevision: 1, snapshot: { ...initial, revision: 1, profileRevision: 1, phase: 'seats', running: true } };
        }, base);
        selectCancellation(page);
        change(page, 'refreshMode', mode);
        change(page, 'features.automaticRefresh', refresh);
        change(page, 'targetCount', 3);
        change(page, 'cancellationDelayMs', 425);
        assert.equal(q(page, '#legacy-zone-settings').hidden, !['legacyRegions', 'legacyKeys'].includes(mode));
        await start(page, trigger);
        assert.equal(payloads.at(-1).features.automatic_refresh, refresh);
        assert.equal(payloads.at(-1).features.region_priority, true, 'Other-mode options are preserved for runtime scoping');
        assert.equal(payloads.at(-1).desktop.refresh_mode, mode === 'internalButton' ? 'button' : mode === 'automatic' ? 'automatic' : 'none');
        assert.equal(payloads.at(-1).legacy.cancellation_navigation.type, mode === 'legacyKeys' ? 'arrow_keys' : mode === 'legacyRegions' ? 'two_regions' : 'none');
        assert.equal(payloads.at(-1).refresh_target === null, !hasTarget);
        assert.deepEqual(payloads.at(-1).legacy.complete, base.legacy.complete);
        assert.deepEqual(payloads.at(-1).legacy.regions, base.legacy.regions);
      }
      assert.deepEqual(payloads[0], payloads[1], `${mode}/${refresh}/${hasTarget}: button and F2 use the same current profile`);
    }
  }
}

// Explicitly selecting two regions never silently sends a previous arrow mode,
// and missing navigation setup does not prevent Save or preparing F8.
for (const trigger of ['button', 'F2']) {
  const arrows = fromForm({ ...toForm(defaultProfile()), refreshMode: 'legacyKeys' }, defaultProfile());
  const page = await loadAppPage(html, initial, async (name, args) => {
    if (name === 'start_shortcut') { assert.equal(args.profile, null); assert.match(args.error, /첫 번째 구역.*두 번째 구역/); return { rejected: true }; }
    assert.ok(['save_profile', 'arm_capture'].includes(name), `Unexpected ${name}`);
    assert.equal(args.profile.legacy.cancellation_navigation.type, 'none');
    return { field: args.field, profile: args.profile, profileRevision: name === 'save_profile' ? 1 : 2 };
  }, arrows);
  selectCancellation(page); change(page, 'refreshMode', 'legacyRegions');
  await start(page, trigger);
  assert.equal(page.calls.some(call => call.name === 'start_run'), false);
  assert.match(q(page, '#feedback').textContent, /첫 번째 구역.*두 번째 구역/);
  await q(page, '#save-button').click();
  assert.equal(q(page, '[data-setting="refreshMode"]').value, 'legacyRegions');
  await q(page, '[data-capture-field="legacy.region_first"]').click();
  assert.equal(page.calls.at(-1).name, 'arm_capture');
}

// F8 can materialize the second region while the editable first region has a
// local change. Stable region identities must preserve both colors and the ROI.
{
  let current;
  const base = fromForm({ ...toForm(defaultProfile()), refreshMode: 'legacyKeys' }, defaultProfile());
  const page = await loadAppPage(html, initial, async (name, args) => {
    if (name === 'save_profile') return { profile: args.profile, profileRevision: 3 };
    assert.equal(name, 'arm_capture'); current = clone(args.profile);
    return { field: args.field, profile: current, profileRevision: 1,
      snapshot: { ...initial, captureState: 'armed', captureArmed: true, captureField: args.field, revision: 1, profileRevision: 1 } };
  }, base);
  assert.equal(q(page, '#legacy-zone-settings').hidden, false);
  await q(page, '[data-capture-field="legacy.regions.1.color"]').click();
  assert.equal(q(page, '[data-setting="zone0ColorEnabled"]').disabled, false);
  change(page, 'zone0ColorEnabled', true); change(page, 'zone0Color', '#00ff00');
  const capture = { field: 'legacy.regions.1.color', color: { r: 0, g: 0, b: 255 }, point: { x: 100, y: 200 }, frame };
  current = applyCapture(current, capture);
  page.emit('point-captured', { ...capture, profile: current, profileRevision: 2 });
  assert.equal(q(page, '[data-setting="zone0ColorEnabled"]').checked, true);
  assert.equal(q(page, '[data-setting="zone1ColorEnabled"]').checked, true);
  await q(page, '#save-button').click();
  const saved = page.calls.at(-1).args.profile;
  assert.deepEqual(clone(saved.legacy.regions[0].color), { r: 0, g: 255, b: 0 });
  assert.deepEqual(clone(saved.legacy.regions[1].color), { r: 0, g: 0, b: 255 });
  assert.deepEqual(clone(saved.color), base.color, 'Zone seat colors never replace common seat color');
  assert.deepEqual(clone(saved.region_color), base.desktop.region_color, 'Zone seat colors never become map region color');
}

// Imported vision objects with no ROI do not fall back in the engine. Avoid a
// misleading common-setting label, while genuinely absent overrides do fall back.
{
  const base = fromForm({ ...toForm(defaultProfile()), refreshMode: 'legacyKeys' }, defaultProfile());
  base.vision.roi = { x: 100, y: 120, width: 500, height: 400 };
  base.legacy.complete = { type: 'point', point: { x: 900, y: 700 } };
  base.legacy.regions = [{ color: null, vision: { ...clone(base.vision), roi: null }, complete: null }];
  const page = await loadAppPage(html, initial, async () => { throw new Error('No mutation'); }, base);
  assert.match(q(page, '[data-point-field="legacy.regions.0.vision.roi.start"]').textContent, /이 구역 좌석 영역 미지정/);
  assert.equal(q(page, '[data-point-field="legacy.regions.1.vision.roi.start"]').textContent, '공통 설정 사용');
  assert.equal(q(page, '[data-point-field="legacy.regions.0.complete"]').textContent, '공통 설정 사용');
  assert.match(q(page, '[data-point-field="legacy.complete"]').textContent, /900.*700/);
  assert.match(q(page, '[data-point-field="vision.roi.start"]').textContent, /100.*120/);
}
console.log('PASS: cancellation audit: button/F2 mode matrix, refresh opt-in retention, missing two-region targets, F8 fixed-slot merge, override labels.');
console.log('Limits: production UI/model in VM DOM with controlled IPC, not native input or site behavior.');
