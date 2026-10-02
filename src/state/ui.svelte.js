let nextToastId = 0;

export const toasts = $state([]);

export function showToast(message, type = "info", timeout = 2200) {
  const id = ++nextToastId;
  toasts.push({ id, message, type });
  setTimeout(() => {
    const index = toasts.findIndex((toast) => toast.id === id);
    if (index >= 0) toasts.splice(index, 1);
  }, timeout);
}

export const ui = $state({
  settingsOpen: false,
  accountPopoverOpen: false,
  addAccountOpen: false,
  smartSetup: null,
  whatsNew: null,
  unsubscribe: null,
  contextMenu: null,
  categoryPopup: null,
  attachmentDownload: null,
  focusSearch: 0,
});

export function closeFloatingMenus() {
  ui.contextMenu = null;
  ui.categoryPopup = null;
}

export function openCategoryPopup(x, y, onSelect) {
  ui.contextMenu = null;
  ui.categoryPopup = { x, y, onSelect };
}
