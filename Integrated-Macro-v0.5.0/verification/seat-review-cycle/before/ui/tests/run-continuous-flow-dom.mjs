import assert from 'node:assert/strict';
import { defaultProfile } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

const profile = defaultProfile();
profile.desktop.capture_points = { legacy_region_first: { x: 10, y: 20 }, legacy_region_second: { x: 30, y: 40 } };
const initial = { phase: 'idle', mode: 'normal', running: false, paused: false, runId: 1, revision: 0,
  captureState: 'idle', manualActions: [], checks: [], selectionPolicy: 'observed' };
const unexpectedInvoke = (name) => { throw new Error(`Unexpected native input: ${name}`); };
const main = await loadAppPage(new URL('../index.html', import.meta.url), initial, unexpectedInvoke, profile);
const overlay = await loadAppPage(new URL('../overlay.html', import.meta.url), initial, unexpectedInvoke, profile);
const $ = (selector) => main.document.querySelector(selector);
const chooseMode = (mode) => {
  for (const radio of main.document.querySelectorAll('input[name="run-mode"]')) radio.checked = radio.value === mode;
  for (const listener of $('input[name="run-mode"]:checked').listeners.change || []) listener();
};
const input = (setting, value) => {
  const field = $(`[data-setting="${setting}"]`);
  if (field.type === 'checkbox') field.checked = value;
  else field.value = value;
  for (const listener of field.listeners.input || []) listener({ target: field });
};
chooseMode('normal');
assert.equal($('#legacy-selection-color-controls').hidden, false);
input('features.captchaResume', true);
assert.equal($('#legacy-selection-color-controls').hidden, true);
assert.equal($('#conflict-recovery-label').textContent, '안내창 자동 대응');
input('features.captchaResume', false);
assert.equal($('#legacy-selection-color-controls').hidden, false);
assert.equal($('[data-setting="features.nolCaptchaDetection"]').checked, false);
input('features.nolCaptchaDetection', true);
assert.equal($('#legacy-selection-color-controls').hidden, true, 'Unsaved NOL-only normal mode uses continuous seat selection');
assert.equal($('#conflict-recovery-label').textContent, '안내창 자동 대응');
assert.equal($('[data-setting="features.captchaResume"]').checked, false, 'NOL detection must not force the Enter option on');
assert.match($('#feature-list').textContent, /NOL 캡챠 판단.*Enter 옵션보다 우선.*문자 자동 풀이는 하지 않습니다/);
assert.match($('#feature-list').textContent, /첫 영문자·숫자 입력.*한 번 저장.*첫 입력만으로 클릭하지 않고/);
assert.match($('#nol-captcha-help').textContent, /Shift 가능.*후속 입력·오답 재입력에도 기준을 유지.*글자 내용은 수집·저장하지 않습니다/);
assert.match($('#feature-list').textContent, /캡챠를 직접 완료한 뒤 F9로 구역·좌석부터 진행/);
const shortcutLabels = new Map($('.shortcut-list').querySelectorAll('div').map((row) => [
  row.querySelector('kbd').textContent, row.querySelector('span').textContent,
]));
assert.equal(shortcutLabels.get('F1'), '일반 예매 시작 · 현재 옵션 적용');
assert.equal(shortcutLabels.get('F5'), '좌석 바로 탐색 · 현재 옵션 적용');
assert.equal(shortcutLabels.get('F9'), '캡챠 이후 진행 · 직접 완료한 뒤');
assert.match($('#captcha-shortcut-help').textContent, /일반 예매 실행 중 캡챠 대기에서만.*캡챠를 직접 완료한 뒤.*진입 버튼을 다시 누르지 않습니다/);
chooseMode('cancellation');
input('refreshMode', 'automatic');
assert.equal($('#legacy-selection-color-controls').hidden, true);
input('refreshMode', 'legacyRegions');
assert.equal($('#legacy-selection-color-controls').hidden, false);
assert.equal($('[data-setting="features.nolCaptchaDetection"]').checked, true, 'Other modes preserve the inactive NOL option');
assert.equal($('#legacy-zone-settings').hidden, false);
assert.match($('[data-coordinate-field="refresh_target"]').textContent, /취소표 새로고침 버튼/);
assert.match($('#feature-list').textContent, /취소표에서 버튼 사용.*일반 예매와 좌석 바로 탐색에는 적용하지 않습니다/);
let revision = 0;
const emit = (change) => {
  const value = { ...initial, revision: ++revision, ...change };
  main.emit('state-change', value);
  overlay.emit('state-change', value);
};
for (const message of ['첫 캡챠 Enter 대기', '재입력한 캡챠 Enter 대기']) {
  emit({ phase: 'captcha', running: true, selectionPolicy: 'optimistic',
    enterWatch: { enabled: true, armed: true, message } });
  assert.equal($('#enter-watch-status').hidden, false);
  assert.equal($('#enter-watch-status').textContent, message);
  assert.equal($('#legacy-selection-color-controls').hidden, true, 'Running snapshot overrides stale form options');
}
emit({ phase: 'captcha', running: true, selectionPolicy: 'optimistic',
  nolCaptcha: { enabled: true, status: 'visible', message: 'NOL 캡챠 창이 닫히기를 기다립니다.' },
  enterWatch: { enabled: true, armed: true, message: '이전 Enter 대기' } });
assert.equal($('#enter-watch-status').hidden, false);
assert.equal($('#enter-watch-status').textContent, 'NOL 캡챠 창이 닫히기를 기다립니다.');
emit({ phase: 'completing', running: true, selectionPolicy: 'optimistic', manualActions: ['completion', 'refresh_completed'],
  pending_action_id: null, enterWatch: { enabled: false, armed: false, message: '' } });
assert.equal($('#manual-confirmation').hidden, true);
assert.equal($('#manual-actions').querySelectorAll('button').length, 0);
assert.equal($('#completion-help').hidden, false);
assert.match($('#completion-help').textContent, /직접 다음 단계를 진행한 뒤 중지/);
assert.equal($('#enter-watch-status').hidden, true);
emit({ phase: 'waiting_refresh', mode: 'cancellation', running: true,
  selectionPolicy: 'observed', manualActions: ['refresh_completed'] });
assert.equal($('#manual-confirmation').hidden, false);
assert.equal($('#manual-actions').querySelector('button').textContent, '새로고침 완료 확인');
assert.match($('#manual-message').textContent, /F6/, 'Original two-region flow retains manual refresh confirmation');
const seatDiagnostic = { phase: 'seats', source: 'global', color: { r: 130, g: 70, b: 210 },
  stats: { matching_pixels: 30, components_found: 3, accepted_count: 3, rejected_too_small: 0, rejected_too_large: 0 } };
const seatState = { phase: 'seats', running: true, selectionPolicy: 'optimistic', estimatedCount: 1,
  selectedCount: 0, targetCount: 2, visionDiagnostics: seatDiagnostic };
emit(seatState);
assert.equal($('#vision-diagnostics').hidden, false);
assert.equal($('#status-symbol').classList.contains('running'), true);
assert.match(overlay.document.querySelector('#overlay-phase').textContent, /색상 후보 3개/);
const waitingMessage = '‘좌석 영역 · 첫 모서리’와 ‘좌석 영역 · 반대 모서리’를 F8로 지정하세요.';
for (const previousPhase of ['region', 'seats']) {
  emit({ ...seatState, waitingForSeatArea: true, message: waitingMessage,
    visionDiagnostics: { ...seatDiagnostic, phase: previousPhase } });
  assert.equal($('#phase-label').textContent, '좌석 영역 미지정 · 지정 대기');
  assert.equal($('#runbar-status').textContent, '좌석 영역 미지정 · 지정 대기');
  assert.equal($('#phase-message').textContent, waitingMessage);
  assert.equal($('#status-symbol').classList.contains('running'), false);
  assert.equal($('#status-symbol').classList.contains('error'), true, 'Use the existing warning color');
  assert.equal($('#runbar-dot').style.background, '#b67659');
  assert.equal($('#vision-diagnostics').hidden, true, 'Neither old region nor old seat candidates describe this wait');
  assert.equal($('#vision-candidates').textContent, '');
  assert.equal($('#start-button').disabled, true);
  assert.equal($('#pause-button').disabled, false);
  assert.equal($('#stop-button').disabled, false);
  assert.equal($('#selected-count').textContent, '1');
  assert.equal(overlay.document.querySelector('#overlay-phase').textContent, '좌석 영역 미지정 · 지정 대기');
  assert.equal(overlay.document.querySelector('#overlay-message').textContent, waitingMessage);
  assert.equal(overlay.document.querySelector('#overlay').classList.contains('error'), true);
  assert.equal(overlay.document.querySelector('#overlay-count').textContent, '선택 추정 1 / 2');
}
emit({ ...seatState, waitingForSeatArea: false });
assert.equal($('#phase-label').textContent, '좌석 탐색');
assert.equal($('#status-symbol').classList.contains('running'), true);
assert.equal($('#status-symbol').classList.contains('error'), false);
assert.equal($('#runbar-dot').style.background, '#589779');
assert.equal($('#vision-diagnostics').hidden, false);
assert.match(overlay.document.querySelector('#overlay-phase').textContent, /색상 후보 3개/);
assert.equal(overlay.document.querySelector('#overlay').classList.contains('error'), false);
assert.equal(overlay.document.querySelector('#overlay-count').textContent, '선택 추정 1 / 2');
for (const mode of ['normal', 'cancellation', 'seats']) {
  emit({ phase: 'completed', mode, running: false, selectionPolicy: 'optimistic', phaseLabel: '좌석 선택 완료',
    manualActions: ['completion'], message: '완료 버튼 클릭 전달 · 실행 종료' });
  assert.equal($('#phase-label').textContent, '실행 종료');
  assert.equal(overlay.document.querySelector('#overlay-phase').textContent, '실행 종료');
  assert.equal($('#manual-confirmation').hidden, true);
  assert.equal($('#completion-help').hidden, true);
}
assert.equal(main.calls.filter(({ name }) => name !== 'get_bootstrap').length, 0);
console.log('PASS: Production continuous flow controls follow runtime policy, preserve two-region settings, show Enter watch and end without completion confirmation.');
console.log('Not verified: native Enter hooks, refresh timing, physical clicks or site outcome.');
