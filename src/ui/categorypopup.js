import { t } from "../lib/i18n.js";

let popup = null;

function closeCategoryPopup() {
    popup?.remove();
    popup = null;
    document.removeEventListener("click", onDocumentClick, true);
    document.removeEventListener("keydown", onKeydown, true);
}

function onDocumentClick(event) {
    if (popup && !popup.contains(event.target)) closeCategoryPopup();
}

function onKeydown(event) {
    if (event.key === "Escape") closeCategoryPopup();
}

window.addEventListener("smart-inbox-enabled", (event) => {
    if (!event.detail?.enabled) closeCategoryPopup();
});

export function showCategoryPopup(categories, x, y, onSelect) {
    closeCategoryPopup();
    if (!categories?.length) return;
    const menu = document.createElement("div");
    menu.className = "category-popup";
    menu.setAttribute("role", "menu");
    menu.innerHTML = `<div class="category-popup-title">${t("smart.choose_category")}</div>`;
    categories.forEach(category => {
        const button = document.createElement("button");
        button.className = "category-popup-item";
        button.setAttribute("role", "menuitem");
        button.innerHTML = `<span class="category-popup-dot" style="--category-color:${category.color || "#6c7065"}"></span>${category.name}`;
        button.addEventListener("click", event => {
            event.stopPropagation();
            closeCategoryPopup();
            onSelect(category);
        });
        menu.appendChild(button);
    });
    document.body.appendChild(menu);
    const rect = menu.getBoundingClientRect();
    menu.style.left = `${Math.max(8, Math.min(x, window.innerWidth - rect.width - 8))}px`;
    menu.style.top = `${Math.max(8, Math.min(y, window.innerHeight - rect.height - 8))}px`;
    popup = menu;
    setTimeout(() => {
        document.addEventListener("click", onDocumentClick, true);
        document.addEventListener("keydown", onKeydown, true);
    }, 0);
}
