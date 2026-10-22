import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { defaultProfile, enterWatchView, fromForm, mergeProfile, nolCaptchaView, normalizeSnapshot, selectionPolicyForProfile, toForm } from '../model.js';

test('profile policy matches normal captcha option and cancellation navigation', () => {
  const profile = defaultProfile();
  assert.equal(selectionPolicyForProfile(profile, 'seats'), 'optimistic');
  assert.equal(selectionPolicyForProfile(profile, 'normal'), 'confirmed');
  profile.features.auto_captcha_resume = true;
  assert.equal(selectionPolicyForProfile(profile, 'normal'), 'optimistic');
  profile.features.auto_captcha_resume = false;
  profile.features.nol_captcha_detection = true;
  assert.equal(selectionPolicyForProfile(profile, 'normal'), 'optimistic');
  assert.equal(selectionPolicyForProfile(profile, 'cancellation'), 'optimistic');
  for (const type of ['two_regions', 'arrow_keys']) {
    profile.legacy.cancellation_navigation = { type };
    assert.equal(selectionPolicyForProfile(profile, 'cancellation'), 'confirmed');
  }
});

test('single-region choices preserve original region coordinates for later use', () => {
  const profile = defaultProfile();
  profile.desktop.capture_points = { legacy_region_first: { x: 10, y: 20 }, legacy_region_second: { x: 30, y: 40 } };
  for (const refreshMode of ['automatic', 'internalButton']) {
    const saved = fromForm({ ...toForm(profile), refreshMode }, profile);
    assert.equal(saved.legacy.cancellation_navigation.type, 'none');
    assert.equal(selectionPolicyForProfile(saved, 'cancellation'), 'optimistic');
    assert.deepEqual(saved.desktop.capture_points, profile.desktop.capture_points);
    const original = fromForm({ ...toForm(saved), refreshMode: 'legacyRegions' }, saved);
    assert.equal(original.legacy.cancellation_navigation.type, 'two_regions');
    assert.equal(selectionPolicyForProfile(original, 'cancellation'), 'confirmed');
  }
});

test('refresh button modes round trip with old and current stored spelling', () => {
  for (const refreshMode of ['button', 'internalButton']) {
    const profile = defaultProfile();
    profile.desktop.refresh_mode = refreshMode;
    profile.features.automatic_refresh = true;
    profile.refresh_target = { type: 'point', point: { x: 30, y: 40 } };
    const form = toForm(profile);
    assert.equal(form.refreshMode, 'internalButton');
    const saved = fromForm(form, profile);
    assert.equal(saved.desktop.refresh_mode, 'button', 'Keep the established stored profile value');
    assert.equal(saved.features.automatic_refresh, true);
    assert.equal(saved.legacy.cancellation_navigation.type, 'none');
    assert.deepEqual(saved.refresh_target, profile.refresh_target);
  }
});

test('imported arrow navigation is shown without requiring desktop metadata', () => {
  const profile = mergeProfile({ legacy: { cancellation_navigation: { type: 'arrow_keys' } } });
  assert.equal(toForm(profile).refreshMode, 'legacyKeys');
  assert.equal(fromForm(toForm(profile), profile).legacy.cancellation_navigation.type, 'arrow_keys');
});

test('completed means execution ended in every mode, not verified booking success', () => {
  for (const mode of ['seats', 'normal', 'cancellation']) {
    const state = normalizeSnapshot({ mode, phase: 'completed', phaseLabel: '좌석 선택 완료' });
    assert.equal(state.label, '실행 종료');
    assert.equal(state.running, false);
  }
});

test('Enter watch uses runtime text and cannot cover paused, capture or terminal state', () => {
  const state = { mode: 'normal', phase: 'captcha', enterWatch: { enabled: true, armed: true, message: '캡챠 Enter 대기' } };
  assert.equal(enterWatchView(state), '캡챠 Enter 대기');
  assert.equal(enterWatchView({ ...state, phase: 'completing' }), '캡챠 Enter 대기', 'Runtime can still accept Enter before completion input is delivered');
  for (const change of [
    { paused: true }, { phase: 'completed' }, { phase: 'stopped' }, { captureState: 'reading' },
    { targetRecovery: { active: true } }, { enterWatch: { enabled: true, armed: false } },
  ]) assert.equal(enterWatchView({ ...state, ...change }), null);
});

test('NOL captcha status takes priority over Enter and only displays during normal entry or captcha', () => {
  const state = { mode: 'normal', phase: 'captcha',
    nolCaptcha: { enabled: true, status: 'visible', message: 'NOL 캡챠 입력 대기' },
    enterWatch: { enabled: true, armed: true, message: '캡챠 Enter 대기' } };
  assert.equal(nolCaptchaView(state), 'NOL 캡챠 입력 대기');
  assert.match(nolCaptchaView({ ...state, nolCaptcha: { enabled: true, status: 'searching' } }),
    /첫 영문자·숫자 입력에서 기준 색을 저장한 뒤 배경 색 변화를 확인/);
  assert.equal(enterWatchView(state), null, 'A stale Enter status must not override enabled NOL detection');
  for (const change of [
    { mode: 'cancellation' }, { mode: 'seats' }, { phase: 'seats' }, { phase: 'completed' },
    { paused: true }, { captureState: 'reading' }, { targetRecovery: { active: true } },
    { nolCaptcha: { enabled: true, status: 'off' } }, { nolCaptcha: { enabled: false, status: 'visible' } },
  ]) assert.equal(nolCaptchaView({ ...state, ...change }), null);
});

test('production controls follow unsaved mode options, Enter watch and final completion without F6', () => {
  const result = spawnSync(process.execPath, [
    '--experimental-vm-modules', fileURLToPath(new URL('./run-continuous-flow-dom.mjs', import.meta.url)),
  ], { encoding: 'utf8', timeout: 15000 });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
  assert.match(result.stdout, /PASS: Production continuous flow controls/);
});
