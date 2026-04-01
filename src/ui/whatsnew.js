import { invoke } from "@tauri-apps/api/core";
import { Store } from "@tauri-apps/plugin-store";
import { t } from "../lib/i18n.js";

let _whatsNewOpen = false;

const CONFETTI_COLORS = [
    '#5aa876',
    '#4CAF50',
    '#FF6B6B',
    '#FFD93D',
    '#6BCB77',
    '#4D96FF',
    '#FF8C42',
    '#A78BFA',
    '#F472B6',
    '#10B981',
    '#EC4899',
    '#F59E0B',
    '#8B5CF6',
    '#06B6D4',
    '#EF4444',
    '#14B8A6',
];

async function wasDismissed(version) {
    try {
        const store = new Store("verdant.json");
        const dismissed = await store.get("whatsNewDismissed");
        if (Array.isArray(dismissed)) {
            return dismissed.includes(version);
        }
    } catch (err) {
        console.error("Failed to check dismissed status:", err);
    }
    return false;
}

async function markAsDismissed(version) {
    try {
        const store = new Store("verdant.json");
        const dismissed = await store.get("whatsNewDismissed") || [];
        if (Array.isArray(dismissed) && !dismissed.includes(version)) {
            dismissed.push(version);
            await store.set("whatsNewDismissed", dismissed);
            await store.save();
        }
    } catch (err) {
        console.error("Failed to mark as dismissed:", err);
    }
}

function createConfetti() {
    const container = document.createElement('div');
    container.className = 'whatsnew-confetti-container';
    
    for (let i = 0; i < 50; i++) {
        const confetti = document.createElement('div');
        confetti.className = 'confetti';
        
        const color = CONFETTI_COLORS[Math.floor(Math.random() * CONFETTI_COLORS.length)];
        const size = Math.random() * 8 + 6;
        const left = Math.random() * 100;
        const delay = Math.random() * 0.2;
        const duration = Math.random() * 2 + 2.5;
        const angle = Math.random() * 360;
        
        confetti.style.left = left + '%';
        confetti.style.top = Math.random() * -20 - 10 + 'px';
        confetti.style.width = size + 'px';
        confetti.style.height = size + 'px';
        confetti.style.background = color;
        confetti.style.borderRadius = Math.random() > 0.5 ? '50%' : '0';
        confetti.style.animation = `confetti-fall ${duration}s linear ${delay}s forwards`;
        confetti.style.transform = `rotate(${angle}deg)`;
        
        container.appendChild(confetti);
    }
    
    document.body.appendChild(container);
    
    setTimeout(() => container.remove(), 5000);
}

export function isWhatsNewOpen() {
    return _whatsNewOpen;
}

function parseMarkdown(text) {
    const escape = str => str.replace(/[&<>"]/g, tag => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'
    }[tag]));
    
    return text
        .replace(/\*\*(.+?)\*\*|__(.+?)__/g, '<strong>$1$2</strong>')
        .replace(/\*(.+?)\*|_(.+?)_/g, '<em>$1$2</em>')
        .replace(/`(.+?)`/g, '<code>$1</code>');
}

export async function openWhatsNewModal(version) {
    if (_whatsNewOpen) return;
    
    if (await wasDismissed(version)) {
        return;
    }

    _whatsNewOpen = true;
    
    createConfetti();

    try {
        const response = await invoke("get_changelog", { version });
        
        const backdrop = document.createElement("div");
        backdrop.className = "whatsnew-backdrop";
        backdrop.onclick = () => closeWhatsNewModal(false);
        document.body.appendChild(backdrop);

        const modal = document.createElement("div");
        modal.className = "whatsnew-modal";
        modal.id = "whatsnew-modal";

        const header = document.createElement("div");
        header.className = "whatsnew-header";
        header.innerHTML = `
            <h2 class="whatsnew-title">${t("whatsnew.title")}</h2>
            <button class="whatsnew-close-btn" aria-label="Close" title="Close">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
        `;
        header.querySelector(".whatsnew-close-btn").onclick = () => closeWhatsNewModal(false);
        modal.appendChild(header);

        const content = document.createElement("div");
        content.className = "whatsnew-content";
        
        const versionDisplay = document.createElement("div");
        versionDisplay.className = "whatsnew-version";
        versionDisplay.textContent = `v${response.version}`;
        content.appendChild(versionDisplay);
        
        const changesList = document.createElement("ul");
        changesList.className = "whatsnew-changes";
        
        let currentBlockquote = null;
        
        response.content.split('\n').forEach(line => {
            const trimmed = line.trim();
            
            if (trimmed.startsWith('> ')) {
                if (!currentBlockquote) {
                    currentBlockquote = document.createElement('blockquote');
                    currentBlockquote.className = 'whatsnew-blockquote';
                    changesList.appendChild(currentBlockquote);
                }
                const p = document.createElement('p');
                p.innerHTML = parseMarkdown(trimmed.substring(2).trim());
                currentBlockquote.appendChild(p);
            }
            else if (trimmed.startsWith('- ')) {
                currentBlockquote = null;
                const li = document.createElement('li');
                li.innerHTML = parseMarkdown(trimmed.substring(2).trim());
                changesList.appendChild(li);
            }
        });
        
        content.appendChild(changesList);
        modal.appendChild(content);

        const footer = document.createElement("div");
        footer.className = "whatsnew-footer";
        footer.innerHTML = `
            <button class="whatsnew-dismiss-btn">${t("whatsnew.dismiss", { default: "Cool" })}</button>
        `;
        footer.querySelector(".whatsnew-dismiss-btn").onclick = () => closeWhatsNewModal(true, version);
        modal.appendChild(footer);

        document.body.appendChild(modal);
    } catch (error) {
        console.error("Failed to load changelog:", error);
        closeWhatsNewModal(false);
    }
}

export function closeWhatsNewModal(shouldMarkDismissed = false, version = null) {
    document.getElementById("whatsnew-modal")?.remove();
    document.querySelector(".whatsnew-backdrop")?.remove();
    _whatsNewOpen = false;
    
    if (shouldMarkDismissed && version) {
        markAsDismissed(version);
    }
}
