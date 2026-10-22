import assert from 'node:assert/strict';
import { defaultProfile } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

const scenario = process.argv[2];
const scenarios = new Set(['normal', 'scheduled', 'cancellation', 'start-failure', 'save-failure', 'missing-entry',
  'seats', 'seats-stale-settings', 'seats-missing-roi', 'cancellation-stale-settings', 'normal-captcha-entry',
  'normal-captcha-coordinates', 'normal-nol-coordinates', 'normal-manual-entry', 'normal-manual-nol']);
assert.ok(scenarios.has(scenario), `Unknown start scenario: ${scenario}`);
const profile = defaultProfile();
const scheduled = scenario === 'scheduled' || scenario === 'missing-entry';
const directSeats = scenario.startsWith('seats');
const mode = directSeats ? 'seats' : scenario.startsWith('cancellation') ? 'cancellation' : 'normal';
const initial = {
  phase: 'idle', runId: 0, revision: 0, mode, running: false, paused: false,
  captureState: 'idle', captureField: '', captureArmed: false, captureError: null,
  message: '', checks: [], manualActions: [], selectedCount: 0, targetCount: 2,
};
if (scheduled) {
  profile.desktop.scheduled_start = true;
  profile.desktop.sale_time = '2027-02-15T11:00:35.000Z';
  if (scenario !== 'missing-entry') {
    profile.entry_target = { type: 'point', point: { x: 240, y: 160 } };
  }
}
if (directSeats && scenario !== 'seats-missing-roi') {
  profile.vision.roi = { x: 100, y: 120, width: 600, height: 500 };
}
if (['seats-stale-settings', 'cancellation-stale-settings'].includes(scenario)) {
  // Direct seats must ignore these earlier workflow settings, not erase them.
  profile.desktop.scheduled_start = true;
  profile.desktop.sale_time = null;
  profile.features.server_clock = true;
  profile.features.auto_entry = true;
  profile.server_url = null;
  profile.entry_target = null;
}
if (scenario === 'normal-captcha-entry') {
  profile.features.auto_entry = true;
  profile.features.auto_captcha_resume = true;
  profile.entry_target = null;
}
if (['normal-captcha-coordinates', 'normal-nol-coordinates'].includes(scenario)) {
  profile.features.auto_captcha_resume = scenario === 'normal-captcha-coordinates';
  profile.features.nol_captcha_detection = scenario === 'normal-nol-coordinates';
  if (scenario === 'normal-nol-coordinates') profile.features.auto_entry = true;
  profile.legacy.date = { type: 'point', point: { x: 130, y: 200 } };
  profile.legacy.session = { type: 'point', point: { x: 250, y: 200 } };
  profile.legacy.next = { type: 'point', point: { x: 400, y: 300 } };
  profile.legacy.normal_click_delay_ms = 350;
}
if (scenario.startsWith('normal-manual-')) {
  profile.features.manual_entry = true;
  profile.features.auto_entry = true;
  profile.features.nol_captcha_detection = scenario === 'normal-manual-nol';
  profile.legacy.date = { type: 'point', point: { x: 130, y: 200 } };
  profile.legacy.session = { type: 'point', point: { x: 250, y: 200 } };
  profile.legacy.next = { type: 'point', point: { x: 400, y: 300 } };
}
const failure = scenario === 'save-failure'
  ? '설정 파일을 저장하지 못했습니다.'
  : '[target_activation_failed] 연결한 예매창을 전면으로 가져오지 못했습니다.';
let saved;
let main;
const invoke = async (name, args) => {
  assert.equal(main.document.querySelector('#start-button').disabled, false, 'A pending start permits a replacement execution');
  assert.equal(name, 'start_run');
  assert.equal(args.expectedProfileRevision, 0);
  saved = JSON.parse(JSON.stringify(args.profile));
  if (scenario === 'save-failure') throw new Error(failure);
  assert.equal(args.mode, mode);
  if (scenario === 'start-failure') throw new Error(failure);
  return { profile: saved, profileRevision: 1, snapshot: {
    ...initial, runId: 1, revision: 1, running: true,
    phase: scheduled ? 'waiting_sale' : 'seats',
    message: scheduled ? '판매 시작 시각 대기 중' : '빈 좌석 계속 탐색 중',
  } };
};
main = await loadAppPage(new URL('../index.html', import.meta.url), initial, invoke, profile);
const $ = (selector) => {
  const element = main.document.querySelector(selector);
  assert.ok(element, `Actual HTML is missing ${selector}`);
  return element;
};
assert.equal($('input[name="run-mode"]:checked').value, 'normal');
assert.match($('#start-button').textContent, /일반 예매 시작/);
const modeRadios = main.document.querySelectorAll('input[name="run-mode"]');
assert.equal(modeRadios.find((radio) => radio.value === 'normal').parent.querySelector('kbd').textContent, 'F1');
assert.equal(modeRadios.find((radio) => radio.value === 'seats').parent.querySelector('kbd').textContent, 'F5');
for (const radio of main.document.querySelectorAll('input[name="run-mode"]')) {
  radio.checked = radio.value === mode;
}
for (const listener of $('input[name="run-mode"]:checked').listeners.change || []) listener();
assert.equal($('#start-button').disabled, false);
await $('#start-button').click();
const calls = main.calls.filter(({ name }) => name !== 'get_bootstrap');
if (scenario === 'seats-missing-roi') {
  assert.deepEqual(calls, []);
  assert.match($('#feedback').textContent, /좌석 바로 탐색.*첫 모서리.*반대 모서리.*F8/);
} else if (scenario === 'missing-entry') {
  assert.deepEqual(calls, []);
  assert.match($('#feedback').textContent, /판매 시작 예매 버튼/);
} else if (scenario === 'save-failure') {
  assert.deepEqual(calls.map(({ name }) => name), ['start_run']);
  assert.equal($('#feedback').textContent, failure);
} else {
  assert.deepEqual(calls.map(({ name }) => name), ['start_run']);
  assert.equal(saved.desktop.scheduled_start, profile.desktop.scheduled_start);
  assert.equal(saved.desktop.sale_time, profile.desktop.sale_time);
  assert.deepEqual(saved.entry_target, profile.entry_target);
  assert.equal(saved.sale_deadline_ms, null, 'The native worker owns the monotonic sale deadline');
  // Hidden-screen coordinates remain optional; known coordinates must survive the Enter option.
  for (const field of ['date', 'session', 'next', 'complete']) {
    assert.deepEqual(saved.legacy[field], profile.legacy[field]);
  }
  assert.equal(saved.features.auto_captcha_resume, profile.features.auto_captcha_resume);
  assert.equal(saved.features.nol_captcha_detection, profile.features.nol_captcha_detection);
  assert.equal(saved.features.manual_entry, profile.features.manual_entry);
  assert.equal(saved.legacy.normal_click_delay_ms, profile.legacy.normal_click_delay_ms);
  assert.deepEqual(saved.vision.roi, profile.vision.roi);
  assert.equal(saved.desktop.region_roi, null);
  assert.equal(saved.refresh_target, null);
  if (directSeats) {
    assert.equal(saved.features.server_clock, profile.features.server_clock);
    assert.equal(saved.features.auto_entry, profile.features.auto_entry);
    assert.match($('#mode-pill').textContent, /좌석 바로 탐색/);
  }
  if (scenario === 'start-failure') {
    assert.equal($('#feedback').textContent, failure);
    assert.ok($('#activity-log').firstElementChild.textContent.includes(failure));
  }
}
const failed = ['start-failure', 'save-failure', 'missing-entry', 'seats-missing-roi'].includes(scenario);
assert.equal($('#status-symbol').classList.contains('running'), !failed);
assert.equal($('#start-button').disabled, false, 'Running and failed executions both permit a new start');
assert.equal($('#pause-button').disabled, failed);
if (failed) {
  assert.equal($('#feedback').hidden, false);
  assert.match($('#phase-label').textContent, /실행 대기/);
} else {
  assert.match($('#phase-label').textContent, scheduled ? /판매 시작 대기/ : /좌석 탐색/);
  assert.equal($('#selected-count').textContent, '0', 'Starting does not report a selected seat');
}
console.log(`PASS: Production start listener ${scenario}.`);
console.log('Not verified: native app deactivation, browser activation, reservation timing, or physical input.');
