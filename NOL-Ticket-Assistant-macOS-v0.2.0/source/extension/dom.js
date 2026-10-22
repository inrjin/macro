/* Shared with local browser fixtures. No network or site-private APIs. */
(() => {
  const normalized = x => String(x || '').replace(/\s+/g, ' ').trim();
  function unique(config, ready = false) {
    let nodes;
    try {
      nodes = [...document.querySelectorAll(ready ? config.readySelector : (config.buttonSelector || 'button,a,[role="button"],input[type="button"],input[type="submit"]'))];
    } catch { throw new Error('CSS 선택자 형식이 올바르지 않습니다.'); }
    const text = normalized(ready ? config.readyText : config.buttonText);
    if (text) nodes = nodes.filter(n => normalized(n.innerText || n.value || n.textContent) === text);
    nodes = nodes.filter(n => visible(n));
    // Ambiguity must never silently select the first button.
    return nodes.length === 1 ? nodes[0] : null;
  }
  function visible(el) {
    if (!el?.isConnected || el.closest('[hidden],[inert],[aria-hidden="true"]')) return false;
    for (let n = el; n; n = n.parentElement) if (Number(getComputedStyle(n).opacity) === 0) return false;
    const r = el.getBoundingClientRect(), s = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && s.display !== 'none' && s.visibility === 'visible' && Number(s.opacity) > 0;
  }
  function enabled(el) {
    if (!visible(el) || el.matches(':disabled') || el.closest('[aria-disabled="true"],[inert]')) return false;
    return getComputedStyle(el).pointerEvents !== 'none';
  }
  function measure(el) {
    if (!enabled(el)) return null;
    const r = el.getBoundingClientRect(), x = r.x + r.width / 2, y = r.y + r.height / 2;
    if (x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) return null;
    const hit = document.elementFromPoint(x, y);
    if (hit !== el && !el.contains(hit)) return null;
    return {x, y};
  }
  function selector(el) {
    if (el.id) { const s = '#' + CSS.escape(el.id); if (document.querySelectorAll(s).length === 1) return s; }
    const path = [];
    for (let n = el; n?.nodeType === 1 && n !== document.documentElement; n = n.parentElement) {
      const siblings = [...n.parentElement.children].filter(s => s.localName === n.localName);
      path.unshift(n.localName + (siblings.length > 1 ? `:nth-of-type(${siblings.indexOf(n) + 1})` : ''));
      const candidate = path.join(' > ');
      if (document.querySelectorAll(candidate).length === 1) return candidate;
    }
    return path.join(' > ');
  }
  globalThis.NolDom = {normalized, unique, visible, enabled, measure, selector};
})();
