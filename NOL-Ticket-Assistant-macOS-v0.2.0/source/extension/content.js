(() => {
  let observer, interval, edgeTimer, session, pending = false, pickerCleanup;
  const D = globalThis.NolDom;
  const send = m => chrome.runtime.sendMessage(m);
  function clear() { observer?.disconnect(); clearInterval(interval); clearTimeout(edgeTimer); observer = null; interval = null; session = null; pending = false; }
  function monitor(s) {
    clear(); session = s;
    const anchored = {wall: Date.now(), monotonic: performance.now()};
    async function check() {
      if (!session || pending) return;
      const now = anchored.wall + performance.now() - anchored.monotonic;
      if (now >= s.expiresAt) { clear(); send({type: 'expired', id: s.id}).catch(() => {}); return; }
      if (document.visibilityState !== 'visible') return;
      try {
        if (s.phase === 'ARMED') {
          if (window !== top || now < s.notBefore || !document.hasFocus()) return;
          const el = D.unique(s.config);
          if (!D.measure(el)) return;
          pending = true;
          const result = await send({type: 'candidate', id: s.id});
          if (!result?.ok) { clear(); return; }
        } else if (s.phase === 'WAIT_READY') {
          const el = D.unique(s.config, true);
          if (!D.enabled(el)) return;
          pending = true;
          const result = await send({type: 'ready', id: s.id});
          if (!result?.ok) clear();
        }
      } catch (e) { clear(); send({type: 'contentError', id: s.id, error: e.message}).catch(() => {}); }
    }
    // DOM events are primary. The low-frequency fallback also catches computed-style changes and timeout.
    observer = new MutationObserver(check);
    observer.observe(document.documentElement, {subtree: true, childList: true, attributes: true, characterData: true});
    interval = setInterval(check, 250);
    if (s.notBefore > anchored.wall) edgeTimer = setTimeout(check, s.notBefore - anchored.wall);
    check();
  }
  function picker(kind) {
    clear(); pickerCleanup?.();
    const banner = document.createElement('div');
    banner.textContent = (kind === 'button' ? '예매 버튼' : '좌석이 준비된 뒤에만 나타나는 요소') + '를 클릭하세요 · Esc 취소';
    banner.style.cssText = 'position:fixed;top:8px;left:8px;z-index:2147483647;padding:12px;background:#162a3b;color:white;font:14px sans-serif;pointer-events:none;border-radius:8px';
    document.documentElement.append(banner);
    const cleanup = () => {banner.remove(); document.removeEventListener('click', pick, true); document.removeEventListener('keydown', key, true);};
    const key = e => { if (e.key === 'Escape') cleanup(); };
    const pick = e => {
      const el = kind === 'button' ? e.target.closest('button,a,[role="button"],input') || e.target : e.target;
      e.preventDefault(); e.stopImmediatePropagation();
      const info = {selector: D.selector(el), text: D.normalized(el.innerText || el.value || el.textContent).slice(0,160), url: location.origin + location.pathname};
      cleanup(); send({type: 'picked', kind, info}).catch(() => {});
    };
    pickerCleanup = cleanup;
    document.addEventListener('click', pick, true); document.addEventListener('keydown', key, true);
  }
  chrome.runtime.onMessage.addListener((m, sender, reply) => {
    if (m.type === 'monitor') { monitor(m.session); reply({ok: true}); }
    if (m.type === 'resync') { clear(); send({type: 'hello'}).then(r => {if(r?.session) monitor(r.session);}).catch(()=>{}); reply({ok:true}); }
    if (m.type === 'clear') { clear(); pickerCleanup?.(); reply({ok: true}); }
    if (m.type === 'pick') { if (m.kind !== 'button' || window === top) picker(m.kind); reply({ok: true}); }
    if (m.type === 'measure') {
      try {
        const point = window === top && document.hasFocus() && document.visibilityState === 'visible' && D.measure(D.unique(m.config));
        reply(point ? {ok: true, ...point} : {ok: false, error: '버튼이 가려졌거나 중복되었거나 창 포커스를 잃었습니다.'});
      } catch (e) { reply({ok: false, error: e.message}); }
    }
    if (m.type === 'readyCheck') {
      try { reply({ok: document.visibilityState === 'visible' && D.enabled(D.unique(m.config, true))}); }
      catch { reply({ok: false}); }
    }
  });
  send({type: 'hello'}).then(r => { if (r?.session) monitor(r.session); }).catch(() => {});
})();
