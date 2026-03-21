function ensureToastWrap() {
  let wrap = document.getElementById("verdant-toast-wrap");
  if (!wrap) {
    wrap = document.createElement("div");
    wrap.id = "verdant-toast-wrap";
    wrap.className = "toast-wrap";
    document.body.appendChild(wrap);
  }
  return wrap;
}

export function showToast(message, type = "info", timeout = 2200) {
  const wrap = ensureToastWrap();
  const toast = document.createElement("div");
  toast.className = `toast ${type}`;
  toast.textContent = message;
  wrap.appendChild(toast);
  setTimeout(() => toast.remove(), timeout);
}
