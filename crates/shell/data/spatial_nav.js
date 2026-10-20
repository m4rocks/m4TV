// Moves focus with the D-pad on pages built for a mouse. Injected into every App unless it opts out.
(() => {
  if (window.m4tvSpatialNav) return;
  window.m4tvSpatialNav = true;
  const TARGETS = 'a[href],button,input,select,textarea,summary,[tabindex]:not([tabindex="-1"]),[role=button],[role=link],[role=tab],[role=menuitem]';
  const DIRS = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  const style = document.createElement('style');
  style.textContent = ':focus{outline:4px solid #fff!important;outline-offset:2px}';
  (document.head || document.documentElement).append(style);
  const centre = (r) => [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
  const usable = (e) => {
    const r = e.getBoundingClientRect();
    return !e.disabled && r.width > 0 && r.height > 0 && r.bottom > 0 && r.right > 0
      && r.top < innerHeight && r.left < innerWidth && getComputedStyle(e).visibility !== 'hidden';
  };
  addEventListener('keydown', (event) => {
    const dir = DIRS[event.key];
    // A page that handled the key itself (YouTube TV does) keeps it.
    if (!dir || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const cur = document.activeElement;
    const typing = cur && (/^(INPUT|TEXTAREA)$/.test(cur.tagName) || cur.isContentEditable);
    if (typing && dir[1] === 0) return; // left and right move the caret
    const from = cur && cur !== document.body ? centre(cur.getBoundingClientRect()) : [innerWidth / 2, innerHeight / 2];
    let best = null, bestScore = Infinity;
    for (const el of document.querySelectorAll(TARGETS)) {
      if (el === cur || !usable(el)) continue;
      const [x, y] = centre(el.getBoundingClientRect());
      const along = (x - from[0]) * dir[0] + (y - from[1]) * dir[1];
      const across = Math.abs((x - from[0]) * dir[1] + (y - from[1]) * dir[0]);
      if (along <= 0) continue;
      const score = along + 2 * across;
      if (score < bestScore) { best = el; bestScore = score; }
    }
    if (!best) return;
    best.focus({ preventScroll: true });
    best.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    event.preventDefault();
  });
})();
