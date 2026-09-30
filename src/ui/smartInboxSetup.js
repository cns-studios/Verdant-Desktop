import {
    previewInboxCategories, categorizeInbox, abortCategorizeInbox, getCategorizeProgress, getInboxCategories,
} from "../api.js";
import { escapeHtml } from "../lib/format.js";
import { t } from "../lib/i18n.js";
import { categoryLabel, setSmartInboxEnabled } from "../lib/smartInbox.js";
import { icon } from "./icons.js";

// Shows the groups this inbox would get *before* anything changes, then fills
// each group's bar as the mail is actually sorted into it. Sorting is fast,
// so the bars are the progress indicator; there is no separate percentage.

function groupRows(groups) {
    const largest = Math.max(1, ...groups.map(g => g.message_count || 0));
    return groups.map((g, i) => `
        <li class="sis-group" style="--group-color:${escapeHtml(g.color || "#7b8075")}; --share:${(g.message_count || 0) / largest}; --i:${i}">
            <span class="sis-dot"></span>
            <span class="sis-name">${escapeHtml(categoryLabel(g))}</span>
            <span class="sis-bar"><i></i></span>
            <span class="sis-count">${escapeHtml(t("smart.setup.count", { n: g.message_count || 0 }))}</span>
        </li>`).join("");
}

export function openSmartInboxSetup({ resort = false, onChanged = () => {} } = {}) {
    document.querySelectorAll(".sis-overlay").forEach(el => el.remove());
    const overlay = document.createElement("div");
    overlay.className = "sis-overlay";
    overlay.innerHTML = `
        <section class="sis-dialog" role="dialog" tabindex="-1" aria-modal="true" aria-labelledby="sis-title">
            <button class="sis-close" aria-label="${escapeHtml(t("reading.close"))}">${icon("x")}</button>
            <h2 id="sis-title"></h2>
            <p class="sis-lede"></p>
            <ul class="sis-groups" aria-live="polite"></ul>
            <p class="sis-note"></p>
            <p class="sis-error" role="alert" hidden></p>
            <div class="sis-actions">
                <button class="sis-primary"></button>
                <button class="sis-secondary"></button>
            </div>
        </section>`;
    document.body.appendChild(overlay);
    requestAnimationFrame(() => overlay.classList.add("open"));

    const $ = sel => overlay.querySelector(sel);
    const list = $(".sis-groups");
    const primary = $(".sis-primary");
    const secondary = $(".sis-secondary");
    let phase = "loading";
    let aborted = false;
    let preview = [];

    const close = () => {
        if (phase === "sorting") return;
        overlay.classList.remove("open");
        setTimeout(() => overlay.remove(), 180);
    };
    $(".sis-close").onclick = close;
    overlay.addEventListener("click", e => { if (e.target === overlay) close(); });
    overlay.addEventListener("keydown", e => { if (e.key === "Escape") close(); });

    const setText = (title, lede, note = "") => {
        $("#sis-title").textContent = title;
        $(".sis-lede").textContent = lede;
        $(".sis-note").textContent = note;
        $(".sis-note").hidden = !note;
    };
    const setError = (message) => {
        $(".sis-error").hidden = !message;
        $(".sis-error").innerHTML = message ? `${icon("alert-circle")}<span>${escapeHtml(message)}</span>` : "";
    };

    function showPreview() {
        phase = "preview";
        overlay.classList.remove("is-sorting", "is-sorted");
        const total = preview.reduce((n, g) => n + (g.message_count || 0), 0);
        if (total === 0) {
            setText(t("smart.setup.title"), t("smart.setup.empty"));
            list.innerHTML = "";
        } else {
            setText(
                t(resort ? "smart.setup.title_again" : "smart.setup.title"),
                t("smart.setup.body"),
                t(resort ? "smart.setup.note_again" : "smart.setup.note"),
            );
            list.innerHTML = groupRows(preview);
        }
        primary.disabled = false;
        primary.textContent = t(total === 0 ? "smart.setup.turn_on" : resort ? "smart.setup.sort_again" : "smart.setup.sort");
        secondary.hidden = false;
        secondary.textContent = t("smart.setup.not_now");
        secondary.onclick = close;
        primary.onclick = sort;
        $(".sis-dialog").focus();
    }

    async function sort() {
        phase = "sorting";
        aborted = false;
        setError("");
        overlay.classList.add("is-sorting");
        primary.disabled = true;
        primary.innerHTML = `<span class="sis-spinner"></span>${escapeHtml(t("smart.setup.sorting"))}`;
        secondary.textContent = t("smart.setup.stop");
        secondary.onclick = async () => {
            aborted = true;
            await abortCategorizeInbox();
        };
        // Large inboxes: show real progress on the bars while it runs.
        const timer = setInterval(async () => {
            try {
                const p = await getCategorizeProgress();
                if (p?.total) list.style.setProperty("--progress", String(Math.min(1, p.processed / p.total)));
            } catch {}
        }, 150);
        try {
            await categorizeInbox();
            clearInterval(timer);
            if (aborted) {
                list.style.removeProperty("--progress");
                showPreview();
                return;
            }
            await setSmartInboxEnabled(true);
            const result = await getInboxCategories();
            showDone(result);
            onChanged();
        } catch (err) {
            clearInterval(timer);
            console.error("Smart inbox sorting failed", err);
            list.style.removeProperty("--progress");
            showPreview();
            setError(t("smart.setup.failed"));
        }
    }

    function showDone(categories) {
        phase = "done";
        list.innerHTML = groupRows(categories);
        list.style.removeProperty("--progress");
        // Next frame, so the bars grow from empty to their real share.
        requestAnimationFrame(() => requestAnimationFrame(() => {
            overlay.classList.remove("is-sorting");
            overlay.classList.add("is-sorted");
        }));
        setText(t("smart.setup.done_title"), t("smart.setup.done_body"));
        primary.disabled = false;
        primary.textContent = t("smart.setup.done");
        primary.onclick = close;
        secondary.hidden = true;
        primary.focus();
    }

    setText(t(resort ? "smart.setup.title_again" : "smart.setup.title"), t("smart.setup.loading"));
    primary.disabled = true;
    primary.textContent = t("smart.setup.sort");
    secondary.textContent = t("smart.setup.not_now");
    secondary.onclick = close;
    previewInboxCategories()
        .then(groups => { preview = groups || []; showPreview(); })
        .catch(err => {
            console.error("Smart inbox preview failed", err);
            preview = [];
            showPreview();
            setError(t("smart.setup.failed"));
        });
}
