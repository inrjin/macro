import {
  FEATURE_OPTIONS, RUN_MODE_LABELS, applyCapture, candidatePositionLabel, cancellationNavigationForMode, captureValue, clearCapture, defaultProfile,
  enterWatchView, fromForm, getPath, keyboardCheckView, mergeProfile, nolCaptchaView, normalizeSnapshot, pointLabel, rgbToHex, selectionPolicyForProfile,
  setPath, snapshotIsCurrent, toForm, validateStart, visionDiagnosticsView, visualDialogView,
} from './model.js';
import { captureRecoveryHint, captureStateFromError, captureStateFromSnapshot } from './capture-state.js';
import { reconcileProfile, sameProfile } from './profile-sync.js';

const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];
const native = Boolean(window.__TAURI__?.core?.invoke);
let profile = defaultProfile();
let baseProfile = structuredClone(profile);
let profileRevision = 0;
let snapshot = normalizeSnapshot({ phase: native ? 'idle' : 'unsupported' });
let bootstrap = {};
let bootstrapReady = false;
let bootstrapError = null;
let commandBusy = false;
let executionRequest = null;
let nativeControlEpoch = 0;
let latestShortcutRequest = 0;
let stopPending = false;
let responseEpoch = 0;
const shortcutRequests = new Set();
let dirty = false;
let selectedCaptureField = null;
let captureStatus = 'idle';
let captureDetail = '';
let captureArmed = false;
let captureFeedbackVisible = false;
let lastMessage = '';
const unlisteners = [];

const COORDINATES = {
  'scheduled-entry-coordinate': [
    ['entry_target', '판매 시작 예매 버튼'],
  ],
  'entry-coordinates': [
    ['legacy.date', '날짜 버튼'],
    ['legacy.session', '회차 버튼'],
    ['legacy.next', '다음 / 예매 버튼'],
    ['legacy.complete', '좌석 선택 완료 버튼'],
  ],
  'seat-coordinates': [
    ['vision.roi.start', '좌석 영역 · 첫 모서리'],
    ['vision.roi.end', '좌석 영역 · 반대 모서리'],
  ],
  'region-coordinates': [
    ['desktop.stage_center', '무대 정면 중앙'],
    ['desktop.region_roi.start', '구역 영역 · 첫 모서리'],
    ['desktop.region_roi.end', '구역 영역 · 반대 모서리'],
  ],
  'refresh-coordinates': [
    ['legacy.region_first', '원본 · 첫 번째 구역'],
    ['legacy.region_second', '원본 · 두 번째 구역'],
    ['refresh_target', '취소표 새로고침 버튼'],
  ],
  'zone0-coordinates': [
    ['legacy.regions.0.vision.roi.start', '1구역 좌석 영역 · 첫 모서리'],
    ['legacy.regions.0.vision.roi.end', '1구역 좌석 영역 · 반대 모서리'],
    ['legacy.regions.0.complete', '1구역 좌석 선택 완료 버튼'],
  ],
  'zone1-coordinates': [
    ['legacy.regions.1.vision.roi.start', '2구역 좌석 영역 · 첫 모서리'],
    ['legacy.regions.1.vision.roi.end', '2구역 좌석 영역 · 반대 모서리'],
    ['legacy.regions.1.complete', '2구역 좌석 선택 완료 버튼'],
  ],
};

function element(tag, className, text) {
  const el = document.createElement(tag);
  if (className) el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}

function buildFeatureRows() {
  for (const [key, title, description] of FEATURE_OPTIONS) {
    const row = element('label', 'feature-row');
    const copy = element('span');
    const heading = element('strong', '', title);
    const help = element('small', '', description);
    if (key === 'conflictRecovery') {
      heading.id = 'conflict-recovery-label';
      help.id = 'conflict-recovery-help';
    }
    copy.append(heading, help);
    const toggle = element('span', 'switch');
    const input = element('input');
    input.type = 'checkbox';
    input.dataset.setting = `features.${key}`;
    toggle.append(input, element('span'));
    row.append(copy, toggle);
    $('#feature-list').append(row);
  }
}

function buildCoordinates() {
  for (const [containerId, fields] of Object.entries(COORDINATES)) {
    const container = document.getElementById(containerId);
    for (const [field, title] of fields) {
      const tile = element('div', 'coordinate-tile');
      tile.dataset.coordinateField = field;
      const output = element('output', '', '미지정');
      output.dataset.pointField = field;
      const capture = element('button', 'text-button', 'F8로 지정');
      capture.type = 'button';
      capture.dataset.native = '';
      capture.dataset.captureField = field;
      const clear = element('button', 'clear-point', '해제');
      clear.type = 'button';
      clear.setAttribute('aria-label', `${title} 지정 해제`);
      clear.addEventListener('click', () => {
        try {
          readForm();
          profile = clearCapture(profile, field);
          renderCoordinates();
          markDirty();
        } catch (error) { feedback(errorText(error), 'error'); }
      });
      tile.append(element('strong', '', title), output, capture, clear);
      container.append(tile);
    }
  }
  const hint = element('p', 'help', '지정할 항목의 F8 버튼을 먼저 누르세요. 선택한 항목만 강조되며, 예매창에서 F8을 누른 순간의 위치를 저장합니다.');
  $('#entry-coordinates').after(hint);
}

function renderForm() {
  const form = toForm(profile);
  for (const input of $$('[data-setting]')) {
    const value = getPath(form, input.dataset.setting);
    if (input.type === 'checkbox') input.checked = Boolean(value);
    else input.value = value ?? '';
  }
  $('#seat-color-text').value = form.seatColor.toUpperCase();
  $('#target-count').textContent = form.targetCount;
  renderCoordinates();
  renderFeatureCount();
  renderZoneSettings();
  renderRunModeChoice();
}

function readForm() {
  const form = toForm(profile);
  for (const input of $$('[data-setting]')) {
    setPath(form, input.dataset.setting, input.type === 'checkbox' ? input.checked : input.value);
  }
  profile = fromForm(form, profile);
  return profile;
}

function renderCoordinates() {
  for (const output of $$('[data-point-field]')) {
    const field = output.dataset.pointField;
    const value = captureValue(profile, field);
    const zoneRange = /^legacy\.regions\.([01])\.vision\.roi\./.exec(field);
    const emptyOverride = zoneRange && profile.legacy.regions[Number(zoneRange[1])]?.vision;
    output.textContent = !value && emptyOverride ? '이 구역 좌석 영역 미지정'
      : !value && field.startsWith('legacy.regions.') ? '공통 설정 사용' : pointLabel(value);
  }
  const selectedColor = profile.desktop?.selected_color;
  $('#selected-color-label').textContent = selectedColor ? rgbToHex(selectedColor).toUpperCase() : '미지정 · 선택 확인을 직접 진행합니다';
}

function renderZoneSettings() {
  const mode = $('[data-setting="refreshMode"]').value;
  $('#legacy-zone-settings').hidden = !['legacyRegions', 'legacyKeys'].includes(mode);
  for (let index = 0; index < 2; index += 1) {
    const enabled = $(`[data-setting="zone${index}ColorEnabled"]`).checked;
    const color = $(`[data-setting="zone${index}Color"]`);
    color.disabled = commandBusy || Boolean(executionRequest) || stopPending || (native && !bootstrapReady) || ['arming', 'reading'].includes(captureStatus) || snapshot.keyboardCheck?.active === true || !enabled || ((snapshot.running || snapshot.paused) && !snapshot.paused);
    $(`#zone${index}-color-label`).textContent = enabled ? color.value.toUpperCase() : '공통 색상 사용';
  }
}

function captureLabel(field) {
  return Object.values(COORDINATES).flat().find(([key]) => key === field)?.[1] || {
    color: '좌석 색상',
    'desktop.region_color': '희망 구역 색상',
    'desktop.selected_color': '선택된 좌석 색상',
    'legacy.regions.0.color': '취소표 1구역 좌석 색상',
    'legacy.regions.1.color': '취소표 2구역 좌석 색상',
  }[field] || '지정 항목';
}

function renderCaptureState() {
  const keyboard = keyboardCheckView(snapshot);
  const checking = keyboard.active || (keyboard.present && !['arming', 'armed', 'reading'].includes(captureStatus));
  const selected = Boolean(selectedCaptureField);
  const armed = captureStatus === 'armed';
  const captured = captureStatus === 'captured';
  const preparing = captureStatus === 'arming';
  const reading = captureStatus === 'reading';
  const failed = captureStatus === 'error';
  const label = captureLabel(selectedCaptureField);
  const stateLabel = captured ? '지정 완료' : preparing ? '지정 준비 중' : reading ? '화면 읽는 중' : armed ? 'F8 입력 대기' : '지정 실패';
  const detail = failed ? `${captureDetail} ${captureRecoveryHint(captureDetail, captureArmed)}` : captureDetail;
  $('#capture-guide').hidden = checking || !selected || captureStatus === 'idle';
  $('#capture-guide').classList.toggle('captured', captured);
  $('#capture-guide').classList.toggle('capture-error', failed);
  $('#capture-guide-title').textContent = selected ? `${label} · ${stateLabel}` : '';
  $('#capture-guide-message').textContent = detail || (armed
    ? '예매창의 원하는 위치에 마우스를 놓고 F8을 누르세요. 다른 항목으로 바꾸려면 해당 항목의 F8 지정 버튼을 누르세요.'
    : preparing ? '현재 설정을 저장하고 예매창의 위치 지정을 준비합니다.' : '');
  $('#capture-status').textContent = checking ? '' : armed ? `${label} · F8 대기` : reading ? `${label} · 화면 읽는 중` : failed ? `${label} · 지정 실패` : captured ? '위치 지정 완료' : '';
  const showCapture = !checking && selected && ['arming', 'armed', 'reading', 'error'].includes(captureStatus);
  $('#phase-label').textContent = showCapture ? `${label} · ${stateLabel}` : snapshot.label;
  $('#phase-message').textContent = showCapture ? $('#capture-guide-message').textContent : snapshot.message || '예매창을 연결하고 준비 상태를 확인하세요.';
  $('#runbar-status').textContent = showCapture ? stateLabel : snapshot.label;
  $('#status-symbol').classList.toggle('error', !checking && (failed || snapshot.phase === 'faulted' || snapshot.waitingForSeatArea));
  $('#runbar-dot').style.background = checking ? '#9aabad' : failed || snapshot.phase === 'faulted' || snapshot.waitingForSeatArea ? '#b67659' : reading || snapshot.running ? '#589779' : '#9aabad';
  for (const tile of $$('[data-coordinate-field]')) {
    const matches = tile.dataset.coordinateField === selectedCaptureField;
    tile.classList.toggle('capture-armed', matches && (armed || preparing || reading));
    tile.classList.toggle('capture-complete', matches && captured);
    tile.classList.toggle('capture-error', matches && failed);
  }
  for (const button of $$('[data-capture-field]')) {
    const matches = button.dataset.captureField === selectedCaptureField;
    button.classList.toggle('capture-armed', matches && (armed || preparing || reading));
    button.classList.toggle('capture-error', matches && failed);
    button.setAttribute('aria-pressed', String(matches && armed));
    const color = button.dataset.captureField.includes('color');
    button.textContent = matches && armed ? 'F8 입력 대기' : matches && reading ? '화면 읽는 중' : matches && failed ? '지정 실패 · 다시 준비' : matches && captured ? '다시 지정 · F8' : color ? 'F8로 색상 지정' : 'F8로 지정';
  }
}

function captureState() {
  return { field: selectedCaptureField, status: captureStatus, detail: captureDetail, armed: captureArmed };
}

function setCaptureState(state) {
  selectedCaptureField = state.field;
  captureStatus = state.status;
  captureDetail = state.detail;
  captureArmed = state.armed;
  if (captureStatus !== 'error' && captureFeedbackVisible) {
    $('#feedback').hidden = true;
    captureFeedbackVisible = false;
  }
}

function renderFeatureCount() {
  const checked = new Set($$('[data-setting^="features."]').filter((input) => input.checked).map((input) => input.dataset.setting));
  $('#enabled-count').textContent = `${checked.size}개 사용`;
}

function markDirty() {
  dirty = true;
  $('#save-status').textContent = '저장하지 않은 변경 사항';
  if (captureStatus === 'captured') {
    captureStatus = 'idle';
    captureDetail = '';
    renderCaptureState();
  }
}

function feedback(message, type = '') {
  const box = $('#feedback');
  box.hidden = false;
  box.textContent = message;
  box.className = `notice ${type}`;
}

function errorText(error) {
  if (typeof error === 'string') return error;
  return error?.message || '요청을 처리하지 못했습니다. 실행 기록을 확인하세요.';
}

function addLog(message, type = '') {
  if (!message || message === lastMessage) return;
  lastMessage = message;
  const list = $('#activity-log');
  list.querySelector('.empty-log')?.remove();
  const item = element('li', type);
  const time = element('time', '', new Date().toLocaleTimeString('ko-KR', { hour12: false }));
  time.dateTime = new Date().toISOString();
  item.append(time, element('span', '', message));
  list.prepend(item);
  while (list.children.length > 60) list.lastElementChild.remove();
}

async function invoke(command, args = {}) {
  if (!native) throw new Error('데스크톱 앱에서 실행하세요. 미리보기에서는 화면 캡처와 입력을 실행하지 않습니다.');
  return window.__TAURI__.core.invoke(command, args);
}

function applyProfileResult(result) {
  if (!result?.profile || !Number.isSafeInteger(result.profileRevision) || result.profileRevision < profileRevision) return false;
  // A revision is adopted only with its authoritative profile. State snapshots
  // can arrive before point-captured and must not bless an older UI draft.
  if (dirty) {
    try { readForm(); } catch { return false; }
  }
  const current = mergeProfile(result.profile);
  const merged = reconcileProfile(baseProfile, profile, current);
  baseProfile = structuredClone(current);
  profile = merged.profile;
  profileRevision = result.profileRevision;
  dirty = !sameProfile(profile, baseProfile);
  renderForm();
  $('#save-status').textContent = dirty ? '저장하지 않은 변경 사항' : '설정 저장됨';
  if (merged.conflicts.length) feedback('동시에 바뀐 항목은 최신 지정값을 유지했습니다. 설정을 확인하세요.', 'warning');
  return true;
}

async function refreshConflictingProfile(epoch = responseEpoch) {
  const current = await invoke('get_bootstrap');
  if (epoch !== responseEpoch) return;
  if (!applyProfileResult(current)) throw new Error('최신 설정을 읽지 못했습니다. 준비 상태 확인을 다시 실행하세요.');
  if (current.snapshot) updateSnapshot(current.snapshot, epoch);
}

async function command(name, args = {}, { quiet = false } = {}) {
  if (name === 'start_run') return executeStart(name, args, { quiet });
  if (commandBusy || executionRequest || stopPending) return undefined;
  if (snapshot.keyboardCheck?.active === true
    && !['stop_keyboard_check', 'stop_run', 'get_bootstrap', 'list_windows'].includes(name)) {
    feedback('키보드 검사를 중지한 뒤 다시 시도하세요.', 'warning');
    return undefined;
  }
  commandBusy = true;
  const epoch = responseEpoch;
  updateControls();
  try {
    const result = await invoke(name, args);
    applyProfileResult(result);
    if (result?.snapshot) updateSnapshot(result.snapshot, epoch);
    else if (typeof result?.phase === 'string') updateSnapshot(result, epoch);
    if (result?.message && !quiet && epoch === responseEpoch) feedback(result.message);
    return result;
  } catch (error) {
    if (epoch !== responseEpoch) return undefined;
    let text = errorText(error);
    const conflict = text.includes('[profile_revision_conflict]');
    if (conflict || ['save_profile', 'arm_capture', 'start_run'].includes(name)) {
      try { await refreshConflictingProfile(epoch); if (conflict) text += ' 최신 지정값을 불러왔습니다. 다시 실행하세요.'; }
      catch (refreshError) { text += ` ${errorText(refreshError)}`; }
    }
    feedback(text, 'error');
    addLog(text, 'error');
    return undefined;
  } finally {
    commandBusy = false;
    updateControls();
  }
}

// Starts can replace starts. Other mutations keep their ordinary lock so that
// a new execution cannot race calibration or overwrite a pending Save.
async function executeStart(name, args, { quiet = false, shortcut = '', controlEpoch } = {}) {
  if (commandBusy || stopPending) return undefined;
  const request = { epoch: ++responseEpoch, controlEpoch: controlEpoch ?? nativeControlEpoch + 1 };
  executionRequest = request;
  updateControls();
  const current = () => executionRequest === request && request.epoch === responseEpoch;
  try {
    const result = await invoke(name, args);
    // Profile revisions are authoritative independently of execution replies.
    applyProfileResult(result);
    const replyEpoch = result?.controlEpoch ?? result?.snapshot?.controlEpoch;
    if (!current() || (Number.isSafeInteger(replyEpoch) && replyEpoch < nativeControlEpoch)) return undefined;
    if (result?.cancelled) return result;
    if (name === 'start_shortcut' && (!result?.profile || !result?.snapshot)) {
      throw new Error(`${shortcut} 시작 결과를 확인하지 못했습니다. 실행 기록을 확인하세요.`);
    }
    if (result?.snapshot) updateSnapshot(result.snapshot, request.epoch);
    else if (typeof result?.phase === 'string') updateSnapshot(result, request.epoch);
    if (result?.message && !quiet && current()) feedback(result.message);
    return result;
  } catch (error) {
    if (!current() || nativeControlEpoch > request.controlEpoch) return undefined;
    let message = errorText(error);
    try {
      await refreshConflictingProfile(request.epoch);
      if (!current() || nativeControlEpoch > request.controlEpoch) return undefined;
      if (message.includes('[profile_revision_conflict]')) message += ' 최신 지정값을 불러왔습니다. 다시 실행하세요.';
    } catch (refreshError) { message += ` ${errorText(refreshError)}`; }
    if (!current() || nativeControlEpoch > request.controlEpoch) return undefined;
    feedback(message, 'error');
    addLog(message, 'error');
    return undefined;
  } finally {
    if (executionRequest === request) executionRequest = null;
    updateControls();
  }
}

async function stopImmediately() {
  if (!native || stopPending) return;
  stopPending = true;
  const epoch = ++responseEpoch;
  updateControls();
  try {
    // Stop bypasses the ordinary command lock. It does not own or release that
    // lock: the older Save/Start may still be awaiting its cancelled response.
    const name = snapshot.keyboardCheck?.active === true ? 'stop_keyboard_check' : 'stop_run';
    const result = await invoke(name);
    applyProfileResult(result);
    if (result?.snapshot) updateSnapshot(result.snapshot, epoch);
    else if (typeof result?.phase === 'string') updateSnapshot(result, epoch);
  } catch (error) {
    const message = errorText(error);
    feedback(message, 'error');
    addLog(message, 'error');
  } finally {
    stopPending = false;
    updateControls();
  }
}

async function startShortcut(payload) {
  const requestId = payload?.requestId;
  const mode = payload?.mode;
  if (!['normal', 'cancellation', 'seats'].includes(mode)) return;
  const shortcut = { normal: 'F1', cancellation: 'F2', seats: 'F5' }[mode];
  if (!Number.isSafeInteger(requestId) || requestId < 1 || requestId <= latestShortcutRequest || shortcutRequests.has(requestId)) return;
  if (Number.isSafeInteger(payload.controlEpoch) && payload.controlEpoch < nativeControlEpoch) return;
  latestShortcutRequest = requestId;
  if (Number.isSafeInteger(payload.controlEpoch)) nativeControlEpoch = Math.max(nativeControlEpoch, payload.controlEpoch);
  shortcutRequests.add(requestId);
  if (shortcutRequests.size > 64) shortcutRequests.delete(shortcutRequests.values().next().value);

  let error = !bootstrapReady
    ? bootstrapError || `설정을 불러오는 중입니다. 준비가 끝난 뒤 ${shortcut} 키를 다시 누르세요.`
    : snapshot.keyboardCheck?.active === true ? '키보드 검사를 중지한 뒤 실행하세요.'
      : (commandBusy || stopPending) ? `다른 요청을 처리 중입니다. 처리가 끝난 뒤 ${shortcut} 키를 다시 누르세요.`
        : ['arming', 'reading'].includes(captureStatus) ? `좌표 지정을 마친 뒤 ${shortcut} 키를 다시 누르세요.` : null;
  if (!error) {
    try {
      readForm();
      validateStart(profile, mode);
    } catch (cause) { error = errorText(cause); }
  }
  if (error) {
    const epoch = responseEpoch;
    const rejectedControlEpoch = nativeControlEpoch;
    const rejectionIsCurrent = () => epoch === responseEpoch && requestId === latestShortcutRequest
      && nativeControlEpoch === rejectedControlEpoch;
    try {
      const result = await invoke('start_shortcut', { requestId, profile: null, expectedProfileRevision: profileRevision, error });
      applyProfileResult(result);
      if (!rejectionIsCurrent()) return;
      if (result?.cancelled) {
        if (result.snapshot) updateSnapshot(result.snapshot, epoch);
        return;
      }
    } catch (cause) { error = errorText(cause); }
    if (!rejectionIsCurrent()) return;
    feedback(error, 'error');
    addLog(error, 'error');
    return;
  }

  await executeStart('start_shortcut', { requestId, profile, expectedProfileRevision: profileRevision, error: null },
    { shortcut, controlEpoch: payload.controlEpoch });
}

function updateControls() {
  const busy = commandBusy || Boolean(executionRequest);
  const recovering = snapshot.targetRecovery?.active === true;
  const checking = snapshot.keyboardCheck?.active === true;
  const captureReading = ['arming', 'reading'].includes(captureStatus);
  const preparing = native && !bootstrapReady;
  const inRun = snapshot.running || snapshot.paused || recovering;
  for (const button of $$('[data-native]')) button.disabled = !native || busy || stopPending || checking || captureReading || preparing;
  $('#start-button').disabled = !native || commandBusy || stopPending || checking || captureReading || preparing;
  $('#pause-button').disabled = !native || busy || stopPending || checking || !inRun;
  $('#stop-button').disabled = !native || stopPending
    || (!busy && !checking && !inRun && !captureArmed && !captureReading && snapshot.phase !== 'faulted');
  $('#stop-button').replaceChildren(document.createTextNode(checking ? '검사 중지 ' : '중지 '), element('kbd', '', 'F4'));
  $('#start-keyboard-check').disabled = !native || busy || stopPending || checking || inRun || captureArmed
    || ['arming', 'reading'].includes(captureStatus) || !bootstrapReady;
  $('#stop-keyboard-check').disabled = !native || stopPending || !checking;
  $('#check-readiness').disabled = !native || busy || stopPending;
  $('#pause-button').replaceChildren(document.createTextNode(snapshot.paused && !recovering ? '다시 진행 ' : '일시정지 '), element('kbd', '', 'F3'));
  $('#save-button').disabled = busy || stopPending || checking || captureReading || preparing || (inRun && !snapshot.paused);
  $('#import-button').disabled = busy || stopPending || checking || captureReading || preparing || inRun;
  for (const input of $$('[data-setting], #seat-color-text, input[name="run-mode"]')) {
    const calibration = ['seatColor', 'regionColor'].includes(input.dataset.setting)
      || /^zone[01]Color(?:Enabled)?$/.test(input.dataset.setting || '') || input.id === 'seat-color-text';
    input.disabled = input.name === 'run-mode'
      ? commandBusy || stopPending || checking || captureReading || preparing
      : busy || stopPending || checking || captureReading || preparing || (inRun && !(snapshot.paused && calibration));
  }
  for (const button of $$('.clear-point')) button.disabled = busy || stopPending || checking || captureReading || preparing || (inRun && !snapshot.paused);
  $('#bind-target').disabled = !native || busy || stopPending || checking || (inRun && !snapshot.paused);
  $('#target-window').disabled = busy || stopPending || checking;
  renderZoneSettings();
  renderRunModeChoice();
}

function renderRunModeChoice() {
  const mode = $('input[name="run-mode"]:checked')?.value || 'normal';
  const label = RUN_MODE_LABELS[mode] || RUN_MODE_LABELS.normal;
  $('#start-button').replaceChildren(document.createTextNode(`${label} 시작 `), element('span', '', '↗'));
  if (snapshot.keyboardCheck?.active === true) $('#mode-pill').textContent = '키보드 검사';
  else if (!snapshot.running && !snapshot.paused && !snapshot.targetRecovery?.active) $('#mode-pill').textContent = label;
  const inRun = snapshot.running || snapshot.paused || snapshot.targetRecovery?.active;
  const previewProfile = {
    ...profile,
    features: { ...profile.features,
      auto_captcha_resume: $('[data-setting="features.captchaResume"]').checked,
      manual_entry: $('[data-setting="features.manualEntry"]').checked,
      nol_captcha_detection: $('[data-setting="features.nolCaptchaDetection"]').checked },
    legacy: { ...profile.legacy,
      cancellation_navigation: cancellationNavigationForMode(profile, $('[data-setting="refreshMode"]').value) },
  };
  const optimistic = inRun ? snapshot.selectionPolicy === 'optimistic'
    : selectionPolicyForProfile(previewProfile, mode) === 'optimistic';
  const countVerification = inRun ? snapshot.selectionCountVerification === true
    : $('[data-setting="features.nolSelectionCount"]')?.checked === true;
  const automaticConflict = optimistic || countVerification;
  $('#legacy-selection-color-controls').hidden = automaticConflict;
  $('#optimistic-selection-help').hidden = !optimistic || countVerification;
  $('#count-selection-help').hidden = !countVerification;
  $('#conflict-recovery-label').textContent = automaticConflict
    ? '안내창 자동 대응' : '선점 실패 시 다음 좌석 시도';
  $('#conflict-recovery-help').textContent = automaticConflict
    ? '좌석 클릭 뒤 중앙 화면 변화 또는 주변 어두워짐을 감지하면 Esc를 보냅니다. 화면이 돌아오면 부족분 탐색을 이어가며, 좌석 선택 완료 버튼 클릭 직전에 감지를 끝냅니다.'
    : '원본 선택 흐름에서는 선점 실패 안내를 직접 확인해 알리면 Esc로 닫고 다음 후보를 시도합니다. 유지된 좌석은 남겨둡니다.';
}

function updateSnapshot(value, epoch) {
  // Replies can share a runtime revision: publish() increments it, not each
  // command. Stop (including F4 state events) invalidates older reply states;
  // their authoritative profiles still reconcile independently by revision.
  if (epoch !== undefined && epoch !== responseEpoch) return;
  if (!snapshotIsCurrent(value, snapshot)) return;
  if (Number.isSafeInteger(value.controlEpoch)) {
    if (value.controlEpoch < nativeControlEpoch) return;
    nativeControlEpoch = value.controlEpoch;
  }
  // point-captured can arrive before an older arm/save reply or state event.
  // Its newer profile proves that those earlier capture states are obsolete.
  if (Number.isSafeInteger(value.profileRevision) && value.profileRevision < profileRevision) return;
  // A new start can publish a stopped cleanup before its replacement is ready.
  // Native control epochs distinguish that from a newer F4 interrupt.
  if (value.phase === 'stopped' && (!executionRequest || !Number.isSafeInteger(value.controlEpoch))) responseEpoch += 1;
  snapshot = normalizeSnapshot(value);
  $('#phase-label').textContent = snapshot.label;
  $('#phase-message').textContent = snapshot.message || '예매창을 연결하고 준비 상태를 확인하세요.';
  $('#selected-count').textContent = snapshot.displaySelectedCount;
  $('#selected-count-label').textContent = snapshot.selectionCountLabel;
  $('#target-count').textContent = snapshot.targetCount;
  const activeRegion = snapshot.active_region ?? snapshot.activeRegion;
  $('#mode-pill').textContent = snapshot.mode === 'cancellation' ? `취소표 탐색${Number.isInteger(activeRegion) ? ` · ${activeRegion + 1}구역` : ''}` : RUN_MODE_LABELS[snapshot.mode] || RUN_MODE_LABELS.seats;
  $('#status-symbol').classList.toggle('running', snapshot.running && !snapshot.paused && !snapshot.waitingForSeatArea && snapshot.keyboardCheck?.active !== true);
  $('#status-symbol').classList.toggle('error', snapshot.phase === 'faulted' || snapshot.waitingForSeatArea);
  $('#runbar-status').textContent = snapshot.label;
  $('#runbar-dot').style.background = snapshot.phase === 'faulted' || snapshot.waitingForSeatArea ? '#b67659' : snapshot.running ? '#589779' : '#9aabad';
  if (snapshot.message && !snapshot.captureError) addLog(snapshot.message, snapshot.phase === 'faulted' ? 'error' : '');
  if (snapshot.checks) renderReadiness(snapshot.checks);
  if ('target' in snapshot || 'boundTarget' in snapshot || 'boundWindow' in snapshot) renderBinding(snapshot.target || snapshot.boundTarget || snapshot.boundWindow);
  if (snapshot.clock) renderClock(snapshot.clock);
  setCaptureState(captureStateFromSnapshot(captureState(), snapshot));
  if (snapshot.captureError && !keyboardCheckView(snapshot).present) {
    feedback(`${captureLabel(selectedCaptureField)} 지정 실패 · ${snapshot.captureError} ${captureRecoveryHint(snapshot.captureError, captureArmed)}`, 'error');
    captureFeedbackVisible = true;
    addLog(`${captureLabel(selectedCaptureField)} 지정 실패 · ${snapshot.captureError}`, 'error');
  }
  renderCaptureState();
  renderKeyboardCheck();
  renderVisionDiagnostics();
  renderManualActions();
  updateControls();
}

function renderKeyboardCheck() {
  const view = keyboardCheckView(snapshot);
  $('#keyboard-check-status').textContent = view.label;
  $('#keyboard-check-message').textContent = view.message;
  $('#keyboard-check-counts').textContent = view.counts;
  $('#keyboard-check-rejections').textContent = view.rejections;
  $('#keyboard-check-rejections').hidden = !view.rejections;
  $('#keyboard-check-environment').textContent = view.environment;
  $('#keyboard-check-remaining').textContent = view.active ? `${view.seconds}초 남음` : '20초 검사';
  $('#keyboard-check-result').classList.toggle('error', view.warning);
}

function renderVisionDiagnostics() {
  const checking = snapshot.keyboardCheck?.active === true;
  const view = checking || snapshot.waitingForSeatArea ? null : visionDiagnosticsView(snapshot.visionDiagnostics);
  $('#vision-diagnostics').hidden = !view;
  $('#vision-heading').textContent = view?.heading || '';
  $('#vision-candidates').textContent = view?.candidates || '';
  $('#vision-settings').textContent = view?.settings || '';
  $('#vision-reason').textContent = view?.reason || '';
  $('#vision-geometry').textContent = view?.geometry || '';
  const dialog = visualDialogView(snapshot);
  $('#visual-dialog-status').hidden = !dialog;
  $('#visual-dialog-status').textContent = dialog?.detail || '';
  const captchaMessage = checking ? null : nolCaptchaView(snapshot) || enterWatchView(snapshot);
  $('#enter-watch-status').hidden = !captchaMessage;
  $('#enter-watch-status').textContent = captchaMessage || '';
  $('#completion-help').hidden = snapshot.phase !== 'completing' || Boolean(snapshot.pending_action_id);
}

function renderReadiness(checks = []) {
  const list = $('#readiness-list');
  list.replaceChildren();
  for (const check of checks) {
    const ok = check.ok ?? check.ready;
    const item = element('li', `check-item ${ok === true ? '' : ok === false ? 'failed' : 'pending'}`);
    const icon = element('span', 'check-icon', ok === true ? '✓' : ok === false ? '!' : '·');
    const copy = element('div');
    copy.append(element('strong', '', check.label || check.name || '준비 상태'));
    copy.append(element('p', '', check.message || check.detail || (ok === true ? '확인되었습니다.' : '확인이 필요합니다.')));
    item.append(icon, copy);
    list.append(item);
  }
  if (!checks.length) {
    const item = element('li', 'check-item pending');
    item.append(element('span', 'check-icon', '·'), element('span', '', '준비 상태를 확인하지 못했습니다.'));
    list.append(item);
  }
}

function renderBinding(target) {
  const bound = Boolean(target?.id || target?.title);
  $('#binding-dot').classList.toggle('connected', bound);
  $('#binding-label').textContent = bound ? `지정된 창: ${target.title || target.id}` : '연결된 창 없음';
}

function renderWindows(windows = []) {
  const select = $('#target-window');
  const previous = select.value;
  select.replaceChildren();
  const empty = element('option', '', windows.length ? '예매창을 선택하세요' : '선택할 수 있는 창이 없습니다');
  empty.value = '';
  select.append(empty);
  for (const target of windows) {
    const option = element('option', '', [target.application, target.title].filter(Boolean).join(' · ') || target.id);
    option.value = String(target.id);
    select.append(option);
  }
  if ([...select.options].some((option) => option.value === previous)) select.value = previous;
}

function renderClock(clock) {
  const offset = clock.offsetMs ?? clock.offset_ms;
  const uncertainty = clock.uncertaintyMs ?? clock.uncertainty_ms;
  if (typeof offset === 'number') {
    $('#clock-status').textContent = `${offset >= 0 ? '+' : ''}${Math.round(offset)} ms${typeof uncertainty === 'number' ? ` (±${Math.round(uncertainty)} ms)` : ''}`;
  } else $('#clock-status').textContent = clock.message || '시간 추정값 없음';
}

const MANUAL_LABELS = {
  captcha_completed: '캡챠 입력 완료',
  dialog_gone: '실패 안내창이 닫힘',
  selection: '좌석 선택 상태 확인',
  refresh_completed: '새로고침 완료 확인',
  conflict: '이미 선점된 좌석 안내 확인',
  seat_map: '좌석 화면 진입 완료',
  region_map_ready: '구역 안의 좌석 화면 표시 완료',
};

function renderManualActions() {
  const optimistic = snapshot.selectionPolicy === 'optimistic';
  const countVerification = snapshot.selectionCountVerification === true;
  const deliveredPolicy = optimistic || countVerification;
  const actions = (snapshot.manualActions || snapshot.manual_actions || []).filter((action) =>
    (typeof action === 'string' ? action : action.kind) !== 'completion'
      && (!countVerification || (typeof action === 'string' ? action : action.kind) !== 'selection')
      && (!optimistic || (typeof action === 'string' ? action : action.kind) !== 'refresh_completed'));
  const container = $('#manual-actions');
  container.replaceChildren();
  $('#manual-confirmation').hidden = snapshot.keyboardCheck?.active === true || !actions.length || !native;
  const hasConflictAction = actions.some((action) => (typeof action === 'string' ? action : action.kind) === 'conflict');
  const optionalConflict = deliveredPolicy && snapshot.visualDialog?.enabled === true
    && actions.length === 1 && hasConflictAction;
  const optionalDialog = deliveredPolicy && snapshot.visualDialog?.enabled === true
    && ['closing', 'waiting_restore'].includes(snapshot.visualDialog.status)
    && actions.length === 1
    && (typeof actions[0] === 'string' ? actions[0] : actions[0].kind) === 'dialog_gone';
  $('#manual-heading').textContent = optionalDialog ? '선택적 수동 확인'
    : optionalConflict ? '직접 확인한 실패 알리기' : '현재 화면 확인';
  $('#manual-kind').textContent = optionalDialog ? '선택 사항' : optionalConflict ? '필요할 때' : '직접 확인';
  let manualMessage = optionalDialog
    ? '안내창의 화면 복귀 확인을 보완하는 선택적 버튼입니다. 예매창이 전면이고 화면 복귀가 감지되면 별도 확인 없이 탐색을 이어갑니다. 안내창이 닫혔음을 직접 확인한 경우에만 사용하세요.'
    : optionalConflict ? '직접 확인한 선점 실패를 보완해서 알릴 수 있습니다.'
    : snapshot.manualMessage || snapshot.manual_message || '예매창에서 실제 결과를 확인한 뒤 아래 버튼 또는 F6으로 알려주세요.';
  if (deliveredPolicy && hasConflictAction) {
    const delivered = snapshot.delivered_attempts || snapshot.deliveredAttempts || [];
    const latest = delivered.at(-1);
    const position = candidatePositionLabel(latest?.candidate_id || latest?.candidateId);
    manualMessage += visualDialogView(snapshot)
      ? ' 안내창 자동 대응을 사용 중입니다. 직접 확인한 실패를 알릴 때 아래 버튼을 사용할 수 있습니다.'
      : snapshot.visualDialog?.enabled === true
        ? ' 안내창 자동 대응은 현재 감시 중이 아닙니다. 직접 확인한 실패를 알릴 때 아래 버튼을 사용할 수 있습니다.'
        : ' 선점 실패 안내는 직접 확인합니다.';
    manualMessage += countVerification
      ? ` ‘직전 클릭 선점 실패 확인’은 마지막으로 입력이 전달된 좌석${position ? ` (${position})` : ''}의 실패를 알립니다. 이후 사이트의 선택 좌석 수를 다시 확인합니다.`
      : ` ‘직전 클릭 선점 실패 확인’은 마지막으로 입력이 전달된 좌석${position ? ` (${position})` : ''}의 선택 추정을 제외합니다. 나머지 선택 추정은 유지합니다.`;
  }
  $('#manual-message').textContent = manualMessage;
  const selectionInputs = [];
  const currentId = snapshot.current_candidate_id || snapshot.currentCandidateId;
  const attempted = snapshot.attempted_ids || snapshot.attemptedIds || [];
  const knownIds = [...new Set([...snapshot.selectedIds, ...attempted, ...(currentId ? [currentId] : [])])];
  let conflictSelect;
  if (!deliveredPolicy && !currentId && hasConflictAction) {
    const label = element('label', 'field');
    label.append(element('span', '', '선점 실패한 좌석 · 예매창 안의 위치'));
    conflictSelect = element('select');
    const placeholder = element('option', '', '실패한 좌석을 확인하고 선택하세요');
    placeholder.value = '';
    conflictSelect.append(placeholder);
    for (const [index, id] of knownIds.entries()) {
      const label = candidatePositionLabel(id);
      const position = label ? ` · ${label}` : '';
      const option = element('option', '', `좌석 ${index + 1}${position}`);
      option.value = id;
      conflictSelect.append(option);
    }
    label.append(conflictSelect);
    container.append(label);
  }
  if (actions.some((action) => (typeof action === 'string' ? action : action.kind) === 'selection')) {
    const ids = knownIds;
    const group = element('div', 'selection-confirmation');
    group.append(element('p', 'help', '현재 예매창에 선택된 좌석만 체크하세요. 선택이 해제된 좌석은 체크를 풀어주세요.'));
    ids.forEach((id, index) => {
      const label = element('label');
      const input = element('input');
      input.type = 'checkbox';
      input.checked = snapshot.selectedIds.includes(id);
      input.value = id;
      selectionInputs.push(input);
      const text = id === currentId ? '방금 클릭한 좌석' : `${snapshot.selectedIds.includes(id) ? '이전에 선택한' : '클릭을 시도한'} 좌석 ${index + 1}`;
      const position = candidatePositionLabel(id);
      label.append(input, element('span', '', `${text}${position ? ` · ${position}` : ''}`));
      group.append(label);
    });
    if (!ids.length) group.append(element('p', 'help', '알려진 선택 좌석이 없습니다. 선택된 좌석이 없다면 아래 확인 버튼으로 진행하세요.'));
    container.append(group);
  }
  for (const action of actions) {
    const spec = typeof action === 'string' ? { kind: action } : action;
    const button = element('button', 'button secondary', deliveredPolicy && spec.kind === 'conflict'
      ? '직전 클릭 선점 실패 확인'
      : optionalDialog && spec.kind === 'dialog_gone' ? '안내창 닫힘 직접 확인 · 선택 사항'
        : spec.label || (spec.kind === 'selection' ? '체크한 좌석 상태 적용' : MANUAL_LABELS[spec.kind]) || spec.kind);
    button.dataset.native = '';
    button.disabled = commandBusy || Boolean(executionRequest) || stopPending || !native;
    button.addEventListener('click', async () => {
      const args = { kind: spec.kind, runId: snapshot.runId, revision: snapshot.revision };
      if (spec.selectedIds || spec.selected_ids) args.selectedIds = spec.selectedIds || spec.selected_ids;
      else if (spec.kind === 'selection') {
        args.selectedIds = selectionInputs.filter((input) => input.checked).map((input) => input.value);
      } else if (spec.kind === 'conflict' && !currentId && !deliveredPolicy) {
        if (!conflictSelect?.value) return feedback('선점 실패한 좌석: 안내와 현재 선택 상태를 확인하고 실패한 좌석을 선택하세요.', 'warning');
        args.selectedIds = [conflictSelect.value];
      }
      await command('confirm_observation', args);
    });
    container.append(button);
  }
}

async function saveProfile({ quiet = false } = {}) {
  if (native && (!bootstrapReady || ['arming', 'reading'].includes(captureStatus))) return false;
  if (snapshot.keyboardCheck?.active === true) {
    feedback('키보드 검사를 중지한 뒤 설정을 저장하세요.', 'warning');
    return false;
  }
  try {
    readForm();
    if (!native) {
      localStorage.setItem('integrated-macro-preview', JSON.stringify(profile));
      dirty = false;
      $('#save-status').textContent = '미리보기 설정 저장됨';
      feedback('이 브라우저의 미리보기 설정을 저장했습니다. 데스크톱 앱의 설정에는 적용되지 않습니다.', 'warning');
      return true;
    }
    const result = await command('save_profile', { profile, expectedProfileRevision: profileRevision }, { quiet });
    if (result === undefined) return false;
    dirty = !sameProfile(profile, baseProfile);
    $('#save-status').textContent = dirty ? '저장하지 않은 변경 사항' : '설정 저장됨';
    if (!quiet) feedback('설정을 저장했습니다.');
    return true;
  } catch (error) {
    feedback(errorText(error), 'error');
    return false;
  }
}

async function refreshWindows() {
  const result = await command('list_windows', {}, { quiet: true });
  if (result) renderWindows(Array.isArray(result) ? result : result.windows || []);
}

async function refreshReadiness() {
  const result = await command('get_bootstrap', {}, { quiet: true });
  if (!result) return;
  bootstrap = result;
  renderReadiness(result.checks || result.snapshot?.checks || []);
  renderBinding(result.target || result.boundTarget || result.snapshot?.boundWindow);
}

async function armCapture(field) {
  if (!native || !bootstrapReady || commandBusy || executionRequest || stopPending || ['arming', 'reading'].includes(captureStatus) || snapshot.keyboardCheck?.active === true) return;
  try {
    readForm();
    selectedCaptureField = field;
    captureStatus = 'arming';
    captureDetail = '';
    captureArmed = false;
    renderCaptureState();
    const result = await command('arm_capture', { field, profile, expectedProfileRevision: profileRevision }, { quiet: true });
    if (result === undefined) {
      captureStatus = 'error';
      captureDetail = '준비하지 못했습니다. 위의 오류 안내를 확인한 뒤 이 항목의 F8 지정 버튼을 다시 누르세요.';
      renderCaptureState();
      return;
    }
    dirty = !sameProfile(profile, baseProfile);
    if (captureStatus === 'arming' || captureStatus === 'armed') {
      $('#save-status').textContent = '설정 저장됨 · F8 지정 대기';
      captureStatus = 'armed';
      captureArmed = true;
      selectedCaptureField = result.field || field;
      captureDetail = '';
    } else if (captureStatus === 'captured') $('#save-status').textContent = '지정한 위치 저장됨';
    renderCaptureState();
    if (captureStatus === 'armed') addLog(`${captureLabel(selectedCaptureField)} · 예매창에서 F8을 누르면 지정합니다.`);
  } catch (error) {
    captureStatus = 'error';
    captureDetail = errorText(error);
    renderCaptureState();
    feedback(errorText(error), 'error');
    addLog(errorText(error), 'error');
  }
}

function receivedCapture(capture) {
  try {
    const hasUnsavedChanges = dirty;
    if (Number.isSafeInteger(capture.profileRevision) && capture.profileRevision < profileRevision) return;
    readForm();
    const field = capture.field || selectedCaptureField;
    if (!field) throw new Error('지정 항목을 확인하지 못했습니다. 원하는 항목의 F8 지정 버튼을 다시 누르세요.');
    if (!applyProfileResult(capture)) {
      profile = applyCapture(profile, { ...capture, field });
      baseProfile = applyCapture(baseProfile, { ...capture, field });
      // A point-only notification can update its coordinate, but cannot
      // establish that the rest of the profile is current.
      renderForm();
    }
    if (hasUnsavedChanges) markDirty();
    else {
      dirty = false;
      $('#save-status').textContent = '지정한 위치 저장됨';
    }
    selectedCaptureField = field;
    captureStatus = 'captured';
    captureArmed = false;
    if (captureFeedbackVisible) {
      $('#feedback').hidden = true;
      captureFeedbackVisible = false;
    }
    captureDetail = `${field.includes('color') ? rgbToHex(capture.color).toUpperCase() : pointLabel(capture.point)} 저장됨. 다시 지정하려면 같은 항목의 F8 지정 버튼을 누르세요.`;
    renderCaptureState();
    addLog(`${captureLabel(field)} 지정 완료 · ${field.includes('color') ? rgbToHex(capture.color).toUpperCase() : pointLabel(capture.point)}`);
  } catch (error) {
    captureStatus = 'error';
    captureDetail = errorText(error);
    renderCaptureState();
    feedback(errorText(error), 'error');
  }
}

function selectTab(tab) {
  for (const button of $$('.nav-item')) {
    const selected = button.dataset.tab === tab;
    button.classList.toggle('active', selected);
    if (selected) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  }
  for (const panel of $$('.tab-panel')) panel.hidden = panel.id !== `panel-${tab}`;
  window.scrollTo({ top: 0, behavior: 'instant' });
}

function attachEvents() {
  for (const button of $$('.nav-item')) button.addEventListener('click', () => selectTab(button.dataset.tab));
  $('.brand').addEventListener('click', () => selectTab('run'));
  for (const input of $$('[data-setting]')) {
    input.addEventListener('input', () => {
      for (const peer of $$('[data-setting]')) {
        if (peer !== input && peer.dataset.setting === input.dataset.setting) {
          if (peer.type === 'checkbox') peer.checked = input.checked;
          else peer.value = input.value;
        }
      }
      if (input.dataset.setting === 'seatColor') $('#seat-color-text').value = input.value.toUpperCase();
      if (input.dataset.setting === 'targetCount' && !snapshot.running) $('#target-count').textContent = input.value;
      renderFeatureCount();
      renderZoneSettings();
      renderRunModeChoice();
      markDirty();
    });
  }
  $('#seat-color-text').addEventListener('change', (event) => {
    if (!/^#[0-9a-f]{6}$/i.test(event.target.value)) {
      feedback('색상 값은 #8246D2처럼 6자리로 입력하세요.', 'error');
      return;
    }
    $('[data-setting="seatColor"]').value = event.target.value;
    markDirty();
  });
  for (const button of $$('[data-capture-field]')) {
    button.addEventListener('click', () => armCapture(button.dataset.captureField));
  }
  $('#clear-selected-color').addEventListener('click', () => {
    profile.desktop.selected_color = null;
    renderCoordinates();
    markDirty();
  });
  $('#save-button').addEventListener('click', () => saveProfile());
  $('#import-button').addEventListener('click', () => $('#import-file').click());
  $('#import-file').addEventListener('change', async (event) => {
    if (snapshot.keyboardCheck?.active === true) { event.target.value = ''; return; }
    const file = event.target.files?.[0];
    if (!file) return;
    const revisionAtRead = profileRevision;
    const runAtRead = snapshot.runId;
    try {
      if (file.size > 1024 * 1024) throw new Error('설정 파일이 너무 큽니다. 1 MB 이하의 JSON 파일을 선택하세요.');
      const text = await file.text();
      if (commandBusy || executionRequest || stopPending || snapshot.running || snapshot.paused || snapshot.keyboardCheck?.active === true
        || ['arming', 'reading'].includes(captureStatus) || revisionAtRead !== profileRevision || runAtRead !== snapshot.runId) {
        feedback('설정이나 실행 상태가 바뀌어 불러오기를 취소했습니다. 필요하면 파일을 다시 선택하세요.', 'warning');
        event.target.value = ''; return;
      }
      profile = mergeProfile(JSON.parse(text));
      renderForm();
      readForm();
      markDirty();
      feedback('설정을 불러왔습니다. 내용을 확인하고 설정 저장을 누르세요.');
    } catch (error) { feedback(errorText(error), 'error'); }
    event.target.value = '';
  });
  $('#refresh-windows').addEventListener('click', refreshWindows);
  $('#check-readiness').addEventListener('click', refreshReadiness);
  $('#start-keyboard-check').addEventListener('click', () => command('start_keyboard_check', {}, { quiet: true }));
  $('#stop-keyboard-check').addEventListener('click', stopImmediately);
  $('#bind-target').addEventListener('click', async () => {
    const id = $('#target-window').value;
    if (!id) return feedback('창 선택: 연결할 예매창을 선택하세요.', 'error');
    const result = await command('bind_target', { id });
    if (result !== undefined) {
      renderBinding(result.target || result.boundTarget || result);
      await refreshReadiness();
    }
  });
  $('#sync-clock').addEventListener('click', async () => {
    try {
      readForm();
      if (!profile.server_url) throw new Error('공연 페이지 주소를 입력한 뒤 시간 측정을 누르세요.');
      if (!await saveProfile({ quiet: true })) return;
      const result = await command('synchronize_clock', { url: profile.server_url });
      if (result) renderClock(result.clock || result);
    } catch (error) { feedback(errorText(error), 'error'); }
  });
  for (const radio of $$('input[name="run-mode"]')) radio.addEventListener('change', renderRunModeChoice);
  $('#start-button').addEventListener('click', async () => {
    if (!bootstrapReady || commandBusy || stopPending || ['arming', 'reading'].includes(captureStatus)) return;
    try {
      const mode = $('input[name="run-mode"]:checked').value;
      readForm();
      validateStart(profile, mode);
      await command('start_run', { mode, profile, expectedProfileRevision: profileRevision });
    } catch (error) {
      // An invalid replacement must not quietly leave its predecessor clicking.
      if (executionRequest || snapshot.running || snapshot.paused) await stopImmediately();
      feedback(errorText(error), 'error');
    }
  });
  $('#pause-button').addEventListener('click', () => command(snapshot.paused && !snapshot.targetRecovery?.active ? 'resume_run' : 'pause_run'));
  $('#stop-button').addEventListener('click', stopImmediately);
  window.addEventListener('beforeunload', () => unlisteners.forEach((unlisten) => unlisten()));
}

async function initialize() {
  buildFeatureRows();
  buildCoordinates();
  attachEvents();
  renderForm();
  updateSnapshot(snapshot);
  $('#preview-banner').hidden = native;
  if (!native) {
    $('#platform-label').textContent = '화면 미리보기';
    $('#save-status').textContent = '데스크톱 설정과 별개';
    try {
      const saved = localStorage.getItem('integrated-macro-preview');
      if (saved) { profile = mergeProfile(JSON.parse(saved)); renderForm(); }
    } catch { /* A corrupt preview setting must not block the desktop interface. */ }
    renderReadiness([
      { label: '데스크톱 앱', message: '현재 화면은 실행 기능이 없는 미리보기입니다.' },
      { label: '화면 캡처 · 실제 입력', message: '운영체제의 권한과 대상 창을 앱에서 확인합니다.' },
      { label: '예매 화면 인식', message: '브라우저 연결 방식이 정해지면 사용할 수 있습니다.' },
    ]);
    updateControls();
    return;
  }
  try {
    if (window.__TAURI__.event?.listen) {
      unlisteners.push(await window.__TAURI__.event.listen('shortcut-start-requested', (event) => startShortcut(event.payload)));
      unlisteners.push(await window.__TAURI__.event.listen('state-change', (event) => updateSnapshot(event.payload)));
      unlisteners.push(await window.__TAURI__.event.listen('point-captured', (event) => receivedCapture(event.payload)));
      unlisteners.push(await window.__TAURI__.event.listen('runtime-error', (event) => {
        const message = errorText(event.payload);
        setCaptureState(captureStateFromError(captureState(), event.payload));
        renderCaptureState();
        feedback(message, 'error');
        captureFeedbackVisible = captureStatus === 'error' && (Boolean(event.payload?.captureField) || event.payload?.captureField === undefined);
        addLog(event.payload?.captureField ? `${captureLabel(event.payload.captureField)} 지정 실패 · ${message}` : message, 'error');
      }));
    }
    bootstrap = await invoke('get_bootstrap');
    if (bootstrap.profile) {
      profile = mergeProfile(bootstrap.profile);
      baseProfile = structuredClone(profile);
      profileRevision = Number.isSafeInteger(bootstrap.profileRevision) ? bootstrap.profileRevision : 0;
      renderForm();
    }
    if (bootstrap.snapshot) updateSnapshot(bootstrap.snapshot);
    if (bootstrap.windows) renderWindows(bootstrap.windows);
    else await refreshWindows();
    renderReadiness(bootstrap.checks || bootstrap.snapshot?.checks || []);
    renderBinding(bootstrap.target || bootstrap.boundTarget || bootstrap.snapshot?.boundWindow);
    $('#platform-label').textContent = bootstrap.platform || navigator.platform || '데스크톱';
    $('#save-status').textContent = '저장된 설정';
    if (bootstrap.clock) renderClock(bootstrap.clock);
    bootstrapReady = true;
  } catch (error) {
    bootstrapError = errorText(error);
    feedback(errorText(error), 'error');
    addLog(errorText(error), 'error');
    $('#save-status').textContent = '앱 연결을 확인하세요';
    updateSnapshot({ phase: 'faulted', message: errorText(error), running: false });
  }
  updateControls();
}

initialize();
