import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { applyCapture, captureValue, defaultProfile, fromForm, mergeProfile, toForm, validateStart } from '../model.js';
import { reconcileProfile } from '../profile-sync.js';

test('fresh cancellation scans the current map while explicit unconfigured two-region mode explains both missing targets', () => {
  const base = defaultProfile();
  assert.equal(toForm(base).refreshMode, 'automatic');
  assert.doesNotThrow(() => validateStart(base, 'cancellation'));
  const arrows = fromForm({ ...toForm(base), refreshMode: 'legacyKeys' }, base);
  const regions = fromForm({ ...toForm(arrows), refreshMode: 'legacyRegions' }, arrows);
  assert.equal(regions.legacy.cancellation_navigation.type, 'none');
  assert.equal(toForm(regions).refreshMode, 'legacyRegions');
  assert.throws(() => validateStart(regions, 'cancellation'), /첫 번째 구역.*두 번째 구역/);
  assert.doesNotThrow(() => fromForm(toForm(regions), regions), 'Incomplete setup can still be saved');
  assert.doesNotThrow(() => validateStart(regions, 'normal'), 'Cancellation setup does not block normal entry');
  const first = applyCapture(regions, { field: 'legacy.region_first', point: { x: 20, y: 30 } });
  assert.throws(() => validateStart(first, 'cancellation'), /두 번째 구역/);
  const both = applyCapture(first, { field: 'legacy.region_second', point: { x: 60, y: 70 } });
  assert.doesNotThrow(() => validateStart(both, 'cancellation'));
});

test('old two-region profiles keep and display their targets across all mode switches without desktop metadata', () => {
  const profile = mergeProfile({ legacy: { cancellation_navigation: {
    type: 'two_regions', first: { type: 'point', point: { x: 20, y: 30 } }, second: { type: 'point', point: { x: 60, y: 70 } },
  } } });
  assert.deepEqual(captureValue(profile, 'legacy.region_first'), profile.legacy.cancellation_navigation.first);
  for (const mode of ['automatic', 'internalButton', 'legacyKeys']) {
    const saved = fromForm({ ...toForm(profile), refreshMode: mode }, profile);
    const restored = fromForm({ ...toForm(saved), refreshMode: 'legacyRegions' }, saved);
    assert.deepEqual(restored.legacy.cancellation_navigation, profile.legacy.cancellation_navigation);
    assert.doesNotThrow(() => validateStart(restored, 'cancellation'));
  }
  const common = mergeProfile({ vision: { roi: { x: 20, y: 30, width: 200, height: 300 } } });
  assert.deepEqual(captureValue(common, 'vision.roi.start'), { x: 20, y: 30 });
  assert.deepEqual(captureValue(common, 'vision.roi.end'), { x: 220, y: 330 });
});

test('newly materialized cancellation region slots reconcile independently without changing arbitrary array policy', () => {
  const empty = () => ({ color: null, vision: null, complete: null });
  const base = { legacy: { regions: [] } };
  const draft = { legacy: { regions: [{ ...empty(), color: { r: 0, g: 255, b: 0 } }] } };
  const current = { legacy: { regions: [empty(), { ...empty(), color: { r: 0, g: 0, b: 255 } }] } };
  const merged = reconcileProfile(base, draft, current);
  assert.deepEqual(merged.profile.legacy.regions[0].color, draft.legacy.regions[0].color);
  assert.deepEqual(merged.profile.legacy.regions[1], current.legacy.regions[1]);
  assert.deepEqual(merged.conflicts, []);
  const rectangle = x => ({ x, y: 1, width: 10, height: 20 });
  const remote = { vision: { exclusions: [rectangle(30), rectangle(40)] } };
  const arbitrary = reconcileProfile({ vision: { exclusions: [] } }, { vision: { exclusions: [rectangle(20)] } }, remote);
  assert.deepEqual(arbitrary.profile, remote);
  assert.deepEqual(arbitrary.conflicts, ['vision.exclusions']);
});

test('production cancellation controls preserve mode/override/refresh contracts across button, F2 and F8', () => {
  const result = spawnSync(process.execPath, ['--experimental-vm-modules', fileURLToPath(new URL('./run-cancellation-audit-dom.mjs', import.meta.url))], { encoding: 'utf8', timeout: 15000 });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout, /PASS: cancellation audit/);
});
