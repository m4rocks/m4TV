// Runs in every App's page before its own scripts. Tells the host whether an editable field has the
// focus, which is when the Remote shows its text box.
const { ipcRenderer } = require("electron");

const NOT_TEXT = /^(button|checkbox|radio|submit|reset|file|image|range|color)$/i;

function focused() {
  let el = document.activeElement;
  // A field inside a web component is hidden behind its shadow root.
  while (el && el.shadowRoot && el.shadowRoot.activeElement) el = el.shadowRoot.activeElement;
  return el;
}

function editable(el) {
  if (!el) return false;
  if (el.isContentEditable || el.tagName === "TEXTAREA") return true;
  return el.tagName === "INPUT" && !NOT_TEXT.test(el.type);
}

let last = false;
function report() {
  const now = editable(focused());
  if (now !== last) ipcRenderer.send("m4tv:editable", (last = now));
}

addEventListener("focusin", report, true);
// The next element is not focused yet while focusout runs.
addEventListener("focusout", () => setTimeout(report), true);
// An off-screen page is not always counted as focused, and then no focus event fires (an autofocus
// field, say), so look now and then as well.
setInterval(report, 250);
