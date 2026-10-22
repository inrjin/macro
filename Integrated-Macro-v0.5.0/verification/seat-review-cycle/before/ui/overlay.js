import { keyboardCheckView, normalizeSnapshot, visionOverlayLabel } from './model.js';
import { captureRecoveryHint } from './capture-state.js';

function render(value) {
  const snapshot = normalizeSnapshot(value);
  const keyboard = keyboardCheckView(snapshot);
  if (keyboard.present) {
    document.getElementById('overlay-phase').textContent = snapshot.label;
    document.getElementById('overlay-message').textContent = `${keyboard.counts} · ${keyboard.rejections || keyboard.message}`;
    document.getElementById('overlay-message').title = `${keyboard.message} · ${keyboard.environment}`;
    document.getElementById('overlay-count').textContent = `${keyboard.accepted}회`;
    document.getElementById('overlay-count').title = '연결한 창의 문자 입력 확인 횟수';
    document.getElementById('overlay-count').hidden = false;
    document.getElementById('overlay').classList.toggle('error', keyboard.warning);
    return;
  }
  const captureFailed = snapshot.captureState === 'error' || Boolean(snapshot.captureError);
  const captureLabels = { armed: 'F8 입력 대기', reading: '위치 읽는 중', error: '위치 지정 실패', captured: '위치 지정 완료' };
  const message = snapshot.captureError || snapshot.message;
  const errorCode = captureFailed ? message.match(/\[([a-z_]+)\]/)?.[1] : null;
  const candidates = visionOverlayLabel(snapshot);
  document.getElementById('overlay-phase').textContent = `${captureLabels[snapshot.captureState] || snapshot.label}${errorCode ? ` · ${errorCode}` : ''}${candidates ? ` · ${candidates}` : ''}`;
  document.getElementById('overlay-message').textContent = captureFailed ? captureRecoveryHint(message, snapshot.captureArmed) : message;
  document.getElementById('overlay-message').title = message;
  document.getElementById('overlay-count').textContent = `${snapshot.selectionPolicy === 'optimistic' ? '선택 추정 ' : ''}${snapshot.displaySelectedCount} / ${snapshot.targetCount}`;
  document.getElementById('overlay-count').title = snapshot.selectionCountLabel;
  document.getElementById('overlay-count').hidden = ['armed', 'reading', 'error', 'captured'].includes(snapshot.captureState);
  document.getElementById('overlay').classList.toggle('error', captureFailed || snapshot.phase === 'faulted' || snapshot.waitingForSeatArea);
}

async function initialize() {
  const tauri = window.__TAURI__;
  if (!tauri?.core?.invoke) {
    render({ phase: 'unsupported', message: '표시창 미리보기 · 실제 실행 상태가 아닙니다.' });
    return;
  }
  await tauri.event.listen('state-change', (event) => render(event.payload));
  try {
    const result = await tauri.core.invoke('get_bootstrap');
    if (result.snapshot) render(result.snapshot);
  } catch (error) {
    render({ phase: 'faulted', message: typeof error === 'string' ? error : error.message });
  }
}

initialize();
