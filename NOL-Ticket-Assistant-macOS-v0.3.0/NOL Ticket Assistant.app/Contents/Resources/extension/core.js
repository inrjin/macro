export const HOSTS = new Set(['nol.yanolja.com', 'tickets.interpark.com', 'ticket.interpark.com', 'poticket.interpark.com']);
export function allowedUrl(value) {
  try { const u = new URL(value); return u.protocol === 'https:' && HOSTS.has(u.hostname) && !u.username && !u.password && !u.port; } catch { return false; }
}
export function samePage(a, b) {
  try { const x = new URL(a), y = new URL(b); return x.origin === y.origin && x.pathname === y.pathname; } catch { return false; }
}
export function parseKst(value) {
  if (!value) return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value);
  if (!m) throw new Error('시각은 한국시간 YYYY-MM-DD HH:MM:SS 형식이어야 합니다.');
  const [year, month, day, hour, minute, second] = m.slice(1).map(x => Number(x || 0));
  const utc = Date.UTC(year, month - 1, day, hour, minute, second);
  const d = new Date(utc);
  if (d.getUTCFullYear() !== year || d.getUTCMonth() !== month - 1 || d.getUTCDate() !== day || hour > 23 || minute > 59 || second > 59) throw new Error('유효하지 않은 시각입니다.');
  return utc - 9 * 3600000;
}
export function validateConfig(c) {
  if (!allowedUrl(c.url) || !/^\/ticket\/products\/\d+\/?$/.test(new URL(c.url).pathname) || new URL(c.url).hostname !== 'nol.yanolja.com') throw new Error('NOL 상품 URL을 입력하세요.');
  if (!c.buttonSelector && !c.buttonText?.trim()) throw new Error('예매 버튼을 지정하세요.');
  if (!Number.isFinite(c.timeoutMinutes) || c.timeoutMinutes < 1 || c.timeoutMinutes > 180) throw new Error('대기는 1~180분입니다.');
  if (c.autoMacro && (!allowedUrl(c.readyUrl) || !c.readySelector?.trim())) throw new Error('자동 매크로에는 좌석 준비 화면 URL과 요소 지정이 필요합니다.');
  parseKst(c.saleAt);
  return c;
}
// HTTP Date has whole-second precision. Cached responses cannot establish the current server clock.
export function clockSample(dateHeader, ageHeader, start, end) {
  if (ageHeader !== null && ageHeader !== undefined && Number(ageHeader) > 0) throw new Error('캐시된 응답입니다. 서버 시각에 사용하지 않습니다.');
  const server = Date.parse(dateHeader);
  if (!Number.isFinite(server) || end < start || end - start > 3000) throw new Error('신뢰할 수 있는 Date 헤더를 받지 못했습니다.');
  const rtt = end - start;
  return {offsetMs: server + 500 - (start + end) / 2, uncertaintyMs: 500 + rtt / 2, rttMs: rtt, sampledAt: end};
}
export function usableClock(sample, now) {
  return !!sample && Number.isFinite(sample.offsetMs) && Number.isFinite(sample.sampledAt) && now - sample.sampledAt >= 0 && now - sample.sampledAt < 600000;
}
export function nextPhase(phase, event) {
  const transitions = {ARMED: {CLICK: 'WAIT_READY', STOP: 'STOPPED'}, WAIT_READY: {START: 'STARTING', STOP: 'STOPPED'}, STARTING: {DONE: 'DONE', STOP: 'STOPPED'}};
  const next = transitions[phase]?.[event];
  if (!next) throw new Error(`허용되지 않는 상태 전환: ${phase}/${event}`);
  return next;
}
