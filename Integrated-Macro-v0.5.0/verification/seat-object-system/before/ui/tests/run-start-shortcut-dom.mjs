import assert from 'node:assert/strict';
import { defaultProfile } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

const scenario = process.argv[2];
const mode = process.argv[3];
const shortcut = { normal: 'F1', cancellation: 'F2', seats: 'F5' }[mode];
const modeLabel = { normal: '일반 예매', cancellation: '취소표 탐색', seats: '좌석 바로 탐색' }[mode];
const profile = defaultProfile();
if (mode === 'cancellation') {
  profile.features.automatic_refresh = true;
  profile.desktop.refresh_mode = 'button';
  profile.desktop.cancellation_mode = 'internalButton';
  profile.refresh_target = { type: 'point', point: { x: 200, y: 100 } };
}
if (mode === 'seats' && scenario !== 'validation') {
  profile.vision.roi = { x: 20, y: 30, width: 400, height: 300 };
}
const initial = { phase: 'idle', mode: 'normal', running: false, paused: false, runId: 0, revision: 0,
  captureState: 'idle', manualActions: [], checks: [], selectedCount: 0, targetCount: 2 };
let release;
let reject;
const gate = new Promise((resolve, fail) => { release = resolve; reject = fail; });
const bootstrapPending = scenario.startsWith('bootstrap-');
const options = bootstrapPending ? { bootstrapWait: gate } : {};
const invoke = async (name, args) => {
  if (name === 'save_profile' && scenario === 'busy') {
    await gate;
    return { profile: args.profile, profileRevision: 1 };
  }
  assert.equal(name, 'start_shortcut');
  assert.equal(args.expectedProfileRevision, 0);
  if (args.error) {
    assert.equal(args.profile, null, 'A rejected shortcut request cannot carry a startable profile');
    return { cancelled: false };
  }
  if (scenario === 'save-failure') throw new Error('설정 파일을 저장하지 못했습니다.');
  if (scenario === 'cancelled') return { cancelled: true };
  if (scenario === 'duplicate') await gate;
  return { profile: args.profile, profileRevision: 1, snapshot: { ...initial, mode, runId: 1, revision: 1, running: true,
    phase: mode === 'normal' ? 'entry' : 'seats', targetCount: args.profile.target_seats, message: `${modeLabel} 시작` } };
};
const page = scenario === 'overlay' ? '../overlay.html' : '../index.html';
const main = await loadAppPage(new URL(page, import.meta.url), initial, invoke, profile, options);
if (scenario === 'overlay') {
  assert.equal(main.listeners['shortcut-start-requested'], undefined);
  console.log(`PASS: ${shortcut} ${mode} shortcut overlay does not handle requests.`);
  process.exit(0);
}
const $ = (selector) => main.document.querySelector(selector);
const request = (requestId, requestedMode = mode) => main.listeners['shortcut-start-requested'][0]({
  payload: { requestId, mode: requestedMode, shortcut: { normal: 'F1', cancellation: 'F2', seats: 'F5' }[requestedMode] },
});
const input = (setting, value) => {
  for (const field of main.document.querySelectorAll(`[data-setting="${setting}"]`)) {
    if (field.type === 'checkbox') field.checked = value;
    else field.value = String(value);
    for (const listener of field.listeners.input || []) listener({ target: field });
  }
};
const calls = () => main.calls.filter(({ name }) => name !== 'get_bootstrap');
assert.equal($('input[name="run-mode"]:checked').value, 'normal');
assert.equal($('[data-setting="features.captchaResume"]').checked, false, 'Shortcuts must not force optional features on');
assert.equal($('[data-setting="features.nolCaptchaDetection"]').checked, false, 'Shortcuts must not force NOL detection on');

if (bootstrapPending) {
  if (scenario === 'bootstrap-failed') {
    reject(new Error('설정을 불러오지 못했습니다.'));
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  await request(1);
  assert.equal(calls().length, 1);
  assert.equal(calls()[0].args.profile, null);
  assert.match(calls()[0].args.error, scenario === 'bootstrap-failed' ? /설정을 불러오지 못했습니다/ : /설정을 불러오는 중/);
  assert.notEqual($('#save-status').textContent, '설정 저장됨');
  if (scenario === 'bootstrap-pending') {
    release();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
} else if (scenario === 'busy') {
  input('targetCount', 3);
  const saving = $('#save-button').click();
  await request(1);
  assert.deepEqual(calls().map(({ name }) => name), ['save_profile', 'start_shortcut']);
  assert.equal(calls()[1].args.profile, null);
  assert.match(calls()[1].args.error, /다른 요청을 처리 중/);
  assert.equal($('#save-button').disabled, true, 'Shortcut rejection cannot release another command lock');
  release();
  await saving;
} else {
  // Native shortcut mode wins even while a different app-button mode is selected.
  const selectedMode = mode === 'cancellation' ? 'normal' : 'cancellation';
  for (const radio of main.document.querySelectorAll('input[name="run-mode"]')) radio.checked = radio.value === selectedMode;
  for (const listener of $('input[name="run-mode"]:checked').listeners.change || []) listener();
  input('features.captchaResume', true);
  input('features.nolCaptchaDetection', ['current-options', 'nol-only-entry'].includes(scenario));
  input('features.regionPriority', true);
  input('targetCount', 3);
  input('clickDelayMs', 375);
  if (mode === 'cancellation') {
    input('features.automaticRefresh', false);
    input('cancellationDelayMs', 425);
  }
  if (scenario === 'validation' && mode === 'normal') input('features.scheduledStart', true);
  if (scenario === 'normal-validation-bypass') {
    input('features.scheduledStart', true);
    input('features.autoEntry', true);
    input('features.serverClock', true);
    assert.equal($('[data-setting="saleAt"]').value, '');
  }
  if (scenario === 'nol-only-entry') {
    input('features.autoEntry', true);
    input('features.captchaResume', false);
  }
  const starting = request(1);
  if (scenario === 'duplicate') {
    await request(1);
    assert.equal(calls().length, 1, 'A duplicate event must not send another start or rejection');
    await request(2, mode === 'normal' ? 'seats' : 'normal');
    assert.equal(calls().length, 2);
    assert.equal(calls()[1].args.profile, null);
    assert.match(calls()[1].args.error, /다른 요청을 처리 중/);
    release();
  }
  await starting;
  const sent = calls()[0];
  assert.equal(sent.name, 'start_shortcut');
  assert.equal(sent.args.requestId, 1);
  if (scenario === 'validation') {
    assert.equal(sent.args.profile, null);
    assert.match(sent.args.error, mode === 'normal' ? /판매 시작 시각/ : /좌석 영역.*첫 모서리/);
  } else {
    assert.equal(sent.args.error, null);
    assert.equal(sent.args.profile.features.auto_captcha_resume, scenario !== 'nol-only-entry');
    assert.equal(sent.args.profile.features.nol_captcha_detection, ['current-options', 'nol-only-entry'].includes(scenario));
    assert.equal(sent.args.profile.features.region_priority, true);
    assert.equal(sent.args.profile.target_seats, 3);
    assert.equal(sent.args.profile.legacy.normal_click_delay_ms, 375);
    assert.equal(sent.args.profile.features.auto_entry, ['normal-validation-bypass', 'nol-only-entry'].includes(scenario));
    assert.equal(sent.args.mode, undefined, 'Backend request token owns the mode');
    if (mode === 'seats') assert.deepEqual({ ...sent.args.profile.vision.roi }, { x: 20, y: 30, width: 400, height: 300 });
    if (mode === 'cancellation') {
      assert.equal(sent.args.profile.features.automatic_refresh, false, 'F2 must apply an unsaved refresh OFF choice');
      assert.equal(sent.args.profile.legacy.cancellation_click_delay_ms, 425);
      assert.equal(sent.args.profile.desktop.refresh_mode, 'button');
      assert.equal(sent.args.profile.refresh_target.point.x, 200, 'Disabling refresh must retain its configured coordinate');
    }
    if (scenario === 'normal-validation-bypass') {
      assert.equal(sent.args.profile.desktop.scheduled_start, true, 'Skipping normal-mode validation must preserve its settings');
      assert.equal(sent.args.profile.features.server_clock, true);
      assert.equal(sent.args.profile.desktop.sale_time, null);
    }
  }
  if (['validation', 'save-failure', 'cancelled'].includes(scenario)) {
    assert.equal($('#save-status').textContent, '저장하지 않은 변경 사항');
    assert.equal($('#status-symbol').classList.contains('running'), false);
    assert.equal($('#start-button').disabled, false);
    if (scenario === 'save-failure') assert.match($('#feedback').textContent, /설정 파일을 저장하지 못했습니다/);
    if (scenario === 'cancelled') assert.equal($('#feedback').hidden, true, 'F4 cancellation is not an error');
  } else {
    assert.equal($('#save-status').textContent, '설정 저장됨');
    assert.equal($('#status-symbol').classList.contains('running'), true);
    assert.equal($('#mode-pill').textContent, modeLabel);
    assert.equal($('input[name="run-mode"]:checked').value, selectedMode, 'A shortcut must not replace the app-button choice');
    assert.equal($('#target-count').textContent, '3');
  }
  await request(1);
  assert.equal(calls().length, scenario === 'duplicate' ? 2 : 1, 'Completed requests also remain deduplicated');
}
assert.equal(calls().some(({ name }) => ['start_run', 'activate_target', 'bind_target'].includes(name)), false);
console.log(`PASS: ${shortcut} ${mode} shortcut ${scenario}.`);
console.log('Not verified: native shortcut delivery, foreground input or disk persistence.');
