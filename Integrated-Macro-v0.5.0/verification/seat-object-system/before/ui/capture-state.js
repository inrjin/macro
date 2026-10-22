const STATES = new Set(['idle', 'armed', 'reading', 'error', 'captured']);

export function captureStateFromSnapshot(current, snapshot) {
  const field = snapshot.captureField || current.field;
  const armed = snapshot.captureArmed ?? current.armed;
  const sameField = field === current.field;
  if (snapshot.captureError) {
    return { field, armed, status: 'error', detail: snapshot.captureError };
  }
  if (STATES.has(snapshot.captureState)) {
    const status = snapshot.captureState;
    const detail = status === 'reading' ? snapshot.message || ''
      : status === 'captured' && sameField && current.status === 'captured' ? current.detail
      : status === 'error' ? snapshot.message || current.detail : '';
    return { field, armed, status, detail };
  }
  // Older snapshots only say whether F8 can be retried. That must not erase a
  // failure received immediately before the snapshot.
  if (armed && sameField && current.status === 'error') return { ...current, armed };
  if (snapshot.captureArmed === true) return { field, armed, status: 'armed', detail: '' };
  if (snapshot.captureArmed === false && ['arming', 'armed', 'reading'].includes(current.status)) {
    return { field, armed, status: 'idle', detail: '' };
  }
  return current;
}

export function captureStateFromError(current, payload) {
  const field = payload?.captureField;
  const legacyCaptureError = field === undefined && ['arming', 'armed', 'reading'].includes(current.status);
  if (!field && !legacyCaptureError) return current;
  return {
    ...current,
    field: field || current.field,
    status: 'error',
    detail: typeof payload === 'string' ? payload : payload?.message || '위치 지정에 실패했습니다.',
  };
}

export function captureRecoveryHint(message, armed) {
  if (message.includes('[accessibility_permission]')) {
    return 'macOS 시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용에서 Integrated Macro를 허용한 뒤, 앱을 다시 실행하고 이 항목의 F8로 지정을 다시 누르세요.';
  }
  if (message.includes('[target_hit_test_unavailable]')) {
    return '연결한 예매창의 본문을 한 번 클릭한 뒤, 대상 위에서 F8을 다시 누르세요. 계속 실패하면 F7로 창을 다시 연결하고 이 항목의 F8로 지정을 다시 누르세요.';
  }
  if (message.includes('[target_focus_unavailable]')) {
    return '예매창의 본문을 클릭하고 F7로 창을 다시 연결한 뒤, 이 항목의 F8로 지정을 다시 누르세요.';
  }
  if (message.includes('[target_not_foreground]')) {
    return '연결한 예매창을 클릭해 맨 앞으로 가져온 뒤, 대상 위에서 F8을 다시 누르세요.';
  }
  if (message.includes('[target_window_changed]')) {
    return '예매창에서 F7로 창을 다시 연결한 뒤, 이 항목의 F8로 지정을 다시 누르세요.';
  }
  if (message.includes('[target_occluded]')) {
    return '대상을 가린 창이나 메뉴를 닫거나 옮기고, 연결한 예매창의 대상 위에서 F8을 다시 누르세요.';
  }
  return armed
    ? '오류 원인을 확인한 뒤 예매창의 대상 위에서 F8을 다시 누르세요.'
    : '오류 원인을 확인한 뒤 이 항목의 F8로 지정을 다시 누르세요.';
}
