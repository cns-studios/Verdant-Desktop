import { icon } from "./icons.js";

const DRAG_THRESHOLD = 6;
const LONG_PRESS_MS = 450;
const ESCAPE_DOUBLE_WINDOW_MS = 400;

let selectedIds = new Set();
let active = false;
let dragActive = false;
let armed = false;
let downX = 0;
let downY = 0;
let suppressClick = false;
let lastEscapeAt = 0;
let pressTimer = null;
let longPressSelected = false;

const listeners = new Set();

function listEl() {
    return document.getElementById("email-list");
}

function notify() {
    listeners.forEach((fn) => {
        try { fn(); } catch (err) { console.error("multiselect listener error", err); }
    });
}

function rowId(row) {
    return row.dataset.threadId || row.dataset.emailId || "";
}

export function isActive() {
    return active;
}

export function selectionCount() {
    return selectedIds.size;
}

export function getSelectedIds() {
    return new Set(selectedIds);
}

export function isRowSelected(row) {
    const id = rowId(row);
    return !!id && selectedIds.has(id);
}

export function getSelectedRows() {
    const el = listEl();
    if (!el) return [];
    return Array.from(el.querySelectorAll(".email-item.selected"));
}

export function checkboxHtml() {
    return `<span class="email-checkbox">${icon("check", 12)}</span>`;
}

export function getMasterState() {
    const el = listEl();
    if (!el) return "none";
    const rows = el.querySelectorAll(".email-item");
    if (rows.length === 0 || selectedIds.size === 0) return "none";
    let selected = 0;
    rows.forEach((row) => { if (isRowSelected(row)) selected++; });
    if (selected === rows.length) return "all";
    return "some";
}

export function refresh(list) {
    const el = list || listEl();
    if (!el) return;
    el.classList.toggle("multi-select", active || selectedIds.size > 0);
    el.querySelectorAll(".email-item").forEach((row) => {
        row.classList.toggle("selected", isRowSelected(row));
    });
    notify();
}

export function clearSelection() {
    selectedIds.clear();
    dragActive = false;
    clearTimeout(pressTimer);
    refresh();
}

export function exitMultiSelect() {
    selectedIds.clear();
    active = false;
    dragActive = false;
    clearTimeout(pressTimer);
    refresh();
}

export function selectAllVisible() {
    const el = listEl();
    if (!el) return;
    active = true;
    el.querySelectorAll(".email-item").forEach((row) => {
        const id = rowId(row);
        if (!id) return;
        selectedIds.add(id);
        row.classList.add("selected");
    });
    refresh();
}

export function deselectAllVisible() {
    const el = listEl();
    if (!el) return;
    el.querySelectorAll(".email-item").forEach((row) => row.classList.remove("selected"));
    selectedIds.clear();
    notify();
}

export function subscribe(fn) {
    listeners.add(fn);
    return () => listeners.delete(fn);
}

function toggleRow(row) {
    const id = rowId(row);
    if (!id) return;
    if (selectedIds.has(id)) {
        selectedIds.delete(id);
        row.classList.remove("selected");
    } else {
        selectedIds.add(id);
        row.classList.add("selected");
    }
    notify();
}

function addRow(row) {
    if (!row) return;
    const id = rowId(row);
    if (!id || selectedIds.has(id)) return;
    selectedIds.add(id);
    row.classList.add("selected");
    notify();
}

function enterSelectMode(row) {
    active = true;
    refresh();
    addRow(row);
}

export function bindMultiSelect() {
    const list = document.getElementById("email-list");
    if (!list) return;

    list.addEventListener("mousedown", (event) => {
        if (event.button !== 0) return;
        armed = true;
        downX = event.clientX;
        downY = event.clientY;
        dragActive = false;
        longPressSelected = false;
        clearTimeout(pressTimer);
        const row = event.target instanceof Element ? event.target.closest(".email-item") : null;
        if (!row) return;
        pressTimer = setTimeout(() => {
            if (dragActive) return;
            longPressSelected = true;
            suppressClick = true;
            enterSelectMode(row);
        }, LONG_PRESS_MS);
    });

    document.addEventListener("mousemove", (event) => {
        if (event.buttons === 0 || !armed) return;
        if (dragActive) {
            const row = document.elementFromPoint(event.clientX, event.clientY)?.closest?.(".email-item");
            if (row) addRow(row);
            return;
        }
        const el = document.elementFromPoint(event.clientX, event.clientY);
        if (!el || !list.contains(el)) return;
        if (Math.abs(event.clientX - downX) > DRAG_THRESHOLD || Math.abs(event.clientY - downY) > DRAG_THRESHOLD) {
            clearTimeout(pressTimer);
            dragActive = true;
            enterSelectMode(null);
        }
    });

    document.addEventListener("mouseup", () => {
        clearTimeout(pressTimer);
        armed = false;
        if (dragActive) {
            suppressClick = true;
            dragActive = false;
            setTimeout(() => { suppressClick = false; }, 0);
        }
    });

    list.addEventListener("click", (event) => {
        if (suppressClick) {
            event.stopPropagation();
            suppressClick = false;
            return;
        }
        if (!active) return;
        const row = event.target instanceof Element ? event.target.closest(".email-item") : null;
        if (!row) return;
        event.stopPropagation();
        toggleRow(row);
    }, true);

    document.addEventListener("keydown", (event) => {
        if (event.key !== "Escape") return;
        if (!active && selectedIds.size === 0) return;
        const now = performance.now();
        if (now - lastEscapeAt < ESCAPE_DOUBLE_WINDOW_MS) {
            exitMultiSelect();
            lastEscapeAt = 0;
        } else {
            lastEscapeAt = now;
            clearSelection();
        }
    });
}