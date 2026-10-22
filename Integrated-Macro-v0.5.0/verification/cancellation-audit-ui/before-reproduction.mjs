// Historical pre-fix reproduction. Reads production sources at execution time;
// after the fix its old-bug assertions should fail. No app/OS input is used.
import assert from 'node:assert/strict';
import { defaultProfile, fromForm, toForm, applyCapture, mergeProfile, captureValue } from '../../ui/model.js';
import { loadAppPage } from '../../ui/tests/dom-fixture.mjs';
const initial = { phase: 'idle', runId: 0, revision: 0, mode: 'cancellation', running: false, paused: false, captureState: 'idle', checks: [], manualActions: [] };
const clone = value => JSON.parse(JSON.stringify(value));
const html = new URL('../../ui/index.html', import.meta.url);
const q = (page, selector) => page.document.querySelector(selector);
function change(page, setting, value) {
  for (const el of page.document.querySelectorAll(`[data-setting="${setting}"]`)) {
    if (el.type === 'checkbox') el.checked = value; else el.value = String(value);
    for (const cb of el.listeners.input || []) cb({ target: el });
  }
}
const arrows = fromForm({ ...toForm(defaultProfile()), refreshMode: 'legacyKeys' }, defaultProfile());
const page = await loadAppPage(html, initial, async (name, args) => {
  assert.equal(name, 'start_run');
  return { profile: args.profile, profileRevision: 1 };
}, arrows);
for (const radio of page.document.querySelectorAll('input[name="run-mode"]')) radio.checked = radio.value === 'cancellation';
change(page, 'refreshMode', 'legacyRegions');
await q(page, '#start-button').click();
const sent = page.calls.find(call => call.name === 'start_run').args.profile;
assert.equal(sent.legacy.cancellation_navigation.type, 'arrow_keys');
assert.equal(q(page, '[data-setting="refreshMode"]').value, 'legacyKeys');
console.log(JSON.stringify({ bug: 'selected two regions sends arrows', selected: 'legacyRegions', sentNavigation: sent.legacy.cancellation_navigation, displayedAfterReply: q(page, '[data-setting="refreshMode"]').value }));

let current;
const zonePage = await loadAppPage(html, initial, async (name, args) => {
  if (name === 'save_profile') return { profile: args.profile, profileRevision: 3 };
  assert.equal(name, 'arm_capture'); current = clone(args.profile);
  return { field: args.field, profile: current, profileRevision: 1,
    snapshot: { ...initial, captureState: 'armed', captureArmed: true, captureField: args.field, revision: 1, profileRevision: 1 } };
}, defaultProfile());
await q(zonePage, '[data-capture-field="legacy.regions.1.color"]').click();
change(zonePage, 'zone0ColorEnabled', true);
change(zonePage, 'zone0Color', '#00ff00');
const capture = { field: 'legacy.regions.1.color', color: { r: 0, g: 0, b: 255 }, point: { x: 100, y: 200 }, frame: { width: 1200, height: 800, scale_x: 1, scale_y: 1 } };
current = applyCapture(current, capture);
zonePage.emit('point-captured', { ...capture, profile: current, profileRevision: 2 });
await q(zonePage, '#save-button').click();
const zoneSaved = zonePage.calls.at(-1).args.profile;
assert.equal(zoneSaved.legacy.regions[0].color, null);
console.log(JSON.stringify({ bug: 'F8 in second zone drops unsaved first-zone override', wantedZone0: { r: 0, g: 255, b: 0 }, savedRegions: zoneSaved.legacy.regions }));

const imported = mergeProfile({ legacy: { cancellation_navigation: { type: 'two_regions', first: { type: 'point', point: { x: 10, y: 20 } }, second: { type: 'point', point: { x: 30, y: 40 } } } } });
const single = fromForm({ ...toForm(imported), refreshMode: 'automatic' }, imported);
const restored = fromForm({ ...toForm(single), refreshMode: 'legacyRegions' }, single);
assert.equal(restored.legacy.cancellation_navigation.type, 'none');
console.log(JSON.stringify({ bug: 'imported legacy targets lost across single-mode roundtrip', before: imported.legacy.cancellation_navigation, after: restored.legacy.cancellation_navigation, displayedFirst: captureValue(imported, 'legacy.region_first') ?? null }));

for (const mode of ['legacyRegions', 'legacyKeys']) {
  const base = defaultProfile();
  base.desktop.refresh_mode = 'button'; base.features.automatic_refresh = true; base.features.region_priority = true;
  base.desktop.capture_points = { legacy_region_first: { x: 10, y: 20 }, legacy_region_second: { x: 30, y: 40 } };
  base.refresh_target = { type: 'point', point: { x: 50, y: 60 } };
  const modePage = await loadAppPage(html, initial, async (name, args) => ({ profile: args.profile, profileRevision: 1 }), base);
  for (const radio of modePage.document.querySelectorAll('input[name="run-mode"]')) radio.checked = radio.value === 'cancellation';
  change(modePage, 'refreshMode', mode);
  await q(modePage, '#start-button').click();
  const sent = modePage.calls.at(-1).args.profile;
  assert.equal(sent.features.automatic_refresh, true);
  assert.equal(sent.features.region_priority, true);
  console.log(JSON.stringify({ switchTo: mode, automaticRefresh: sent.features.automatic_refresh, regionPriority: sent.features.region_priority, desktopRefreshMode: sent.desktop.refresh_mode, nav: sent.legacy.cancellation_navigation.type, refreshTarget: sent.refresh_target }));
}
console.log('Limits: actual UI/model sources in VM DOM; controlled IPC, no live Tauri, OS input, or user profile writes.');
