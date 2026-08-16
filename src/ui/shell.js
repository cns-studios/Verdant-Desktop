import { t } from "../lib/i18n.js";
import { icon } from "./icons.js";

export function renderShell() {
  const root = document.getElementById("root");
  root.innerHTML = `
    <header class="app-header" id="app-header">
      <div class="app-header-left" id="app-header-left">
        <span class="app-logo-mark">${icon("mail")}</span>
        <span class="app-title">${t("app.title")}</span>
        <span class="app-subtitle">- ${t("sidebar.inbox")}</span>
      </div>
      <div class="app-header-controls" id="app-header-controls">
        <button class="app-win-btn" id="app-min-btn" aria-label="${t("app.minimize_window")}" title="${t("app.minimize")}">
          ${icon("minus")}
        </button>
        <button class="app-win-btn" id="app-max-btn" aria-label="${t("app.maximize_window")}" title="${t("app.maximize")}">
          ${icon("square")}
        </button>
        <button class="app-win-btn close" id="app-close-btn" aria-label="${t("app.close_window")}" title="${t("app.close")}">
          ${icon("x")}
        </button>
      </div>
    </header>

    <div class="app-content">
      <aside class="sidebar">
        <div class="sidebar-header">
          <div class="sidebar-menu-label">${t("sidebar.mailboxes")}</div>
        </div>

        <div class="sidebar-section">
          <div class="nav-item active" data-mailbox="INBOX">
            ${icon("inbox")}
            ${t("sidebar.inbox")}
          </div>
          <div class="nav-item" data-mailbox="STARRED">
            ${icon("star")}
            ${t("sidebar.starred")}
          </div>
          <div class="nav-item" data-mailbox="ARCHIVE">
            ${icon("archive")}
            ${t("sidebar.archive")}
          </div>
          <div class="nav-item" data-mailbox="SENT">
            ${icon("send")}
            ${t("sidebar.sent")}
          </div>
          <div class="nav-item" data-mailbox="DRAFT">
            ${icon("file-text")}
            ${t("sidebar.drafts")}
          </div>
          <div class="nav-item" data-mailbox="TRASH">
            ${icon("trash")}
            ${t("sidebar.trash")}
          </div>
        </div>

        <div class="compose-wrap">
          <button class="compose-btn" id="compose-open-btn">
            ${icon("plus")}
            ${t("sidebar.compose")}
          </button>
        </div>

        <div class="sidebar-footer">
          <div class="user-row" id="user-row">
            <div class="avatar" id="user-avatar">?</div>
            <div class="user-info">
              <div class="user-name" id="user-name">${t("app.version_loading")}</div>
              <div class="user-email" id="user-email"></div>
            </div>
          </div>
        </div>
      </aside>

      <div class="email-list-pane">
        <div class="list-header">
          <div class="list-title-row">
            <span class="list-title">${t("sidebar.inbox")}</span>
            <span class="list-count">0 ${t("list.count", { n: 0 })}</span>
          </div>
          <div class="search-bar">
            ${icon("search")}
            <input type="text" placeholder="${t("list.search.placeholder")}" id="search-input">
          </div>
          <div class="filter-chips">
            <div class="chip active" data-filter="Important">${t("list.filter.important")}</div>
            <div class="chip" data-filter="All">${t("list.filter.all")}</div>
            <div class="chip" data-filter="Unread">${t("list.filter.unread")}</div>
            <div class="chip" data-filter="Attachments">${t("list.filter.attachments")}</div>
          </div>
        </div>

        <div class="list-sync-bar" id="list-sync-bar">
          <div class="list-sync-bar-inner"></div>
        </div>

        <div class="email-list" id="email-list"></div>
      </div>

      <div class="pane-resizer" id="pane-resizer" role="separator" aria-orientation="vertical" aria-label="${t("list.resize_label")}"></div>

      <div class="reading-pane">
        <div class="reading-header">
          <div class="reading-actions">
            <button class="icon-btn" data-action="archive" title="${t("reading.archive")}">
              ${icon("archive")}
            </button>
            <button class="icon-btn" data-action="delete" title="${t("reading.delete")}">
              ${icon("trash")}
            </button>
            <button class="icon-btn" data-action="mark_unread" title="${t("reading.mark_unread")}">
              ${icon("mail")}
            </button>
            <button class="icon-btn" data-action="star" title="${t("reading.star")}">
              ${icon("star")}
            </button>
            <button class="unsubscribe-btn" data-action="unsubscribe" style="display:none">${t("reading.unsubscribe")}</button>
            <button class="icon-btn" data-action="more" title="${t("reading.more")}" style="margin-left:auto">
              ${icon("dots")}
            </button>
            <button class="icon-btn" data-action="close" title="${t("reading.close")}" aria-label="${t("reading.close")}">
              ${icon("x")}
            </button>
          </div>

          <div class="reading-subject"></div>

          <div class="reading-meta">
            <div class="meta-avatar"></div>
            <div class="meta-info">
              <div class="meta-from"></div>
              <div class="meta-to"></div>
            </div>
            <div class="meta-date"></div>
          </div>
        </div>

        <div class="reading-body">
          <div class="email-body-text"></div>
        </div>
      </div>
    </div>

    <div class="modal-overlay" id="composeModal">
      <div class="compose-modal">
        <div class="modal-header">
          <span class="modal-title">${t("compose.title")}</span>
          <div class="modal-header-actions">
            <button class="modal-close" id="compose-max-btn" title="${t("app.maximize")}" aria-label="${t("app.maximize")}">
              ${icon("square")}
            </button>
            <button class="modal-close" id="compose-close-btn">×</button>
          </div>
        </div>
        <div class="modal-fields">
          <div class="modal-field">
            <label>${t("compose.to")}</label>
            <div class="compose-recipient-wrap">
              <div class="compose-recipient-input" id="compose-to-input-wrap">
                <input id="compose-to" type="text" placeholder="${t("compose.recipient_placeholder")}" autocomplete="off">
              </div>
              <div class="compose-recipient-suggest" id="compose-to-suggest"></div>
            </div>
          </div>
          <div class="modal-field">
            <label>${t("compose.cc")}</label>
            <div class="compose-recipient-wrap">
              <div class="compose-recipient-input" id="compose-cc-input-wrap">
                <input id="compose-cc" type="text" placeholder="${t("compose.cc_placeholder")}" autocomplete="off">
              </div>
              <div class="compose-recipient-suggest" id="compose-cc-suggest"></div>
            </div>
          </div>
          <div class="modal-field">
            <label>${t("compose.subject")}</label>
            <input id="compose-subject" type="text" placeholder="${t("compose.subject_placeholder")}">
          </div>
        </div>
        <div class="modal-body">
          <div id="compose-body" class="compose-editor" contenteditable="true" data-placeholder="${t("compose.placeholder")}"></div>
        </div>
        <div class="compose-format-toolbar" id="compose-format-toolbar">
          <button class="compose-format-btn" type="button" data-format="bold">${t("compose.format.bold")}</button>
          <button class="compose-format-btn" type="button" data-format="header">${t("compose.format.header")}</button>
          <button class="compose-format-btn" type="button" data-format="italic">${t("compose.format.italic")}</button>
          <button class="compose-format-btn" type="button" data-format="list">${t("compose.format.list")}</button>
          <button class="compose-format-btn" type="button" data-format="quote">${t("compose.format.quote")}</button>
          <button class="compose-format-btn" type="button" data-format="code">${t("compose.format.code")}</button>
          <button class="compose-format-btn" type="button" data-format="clear">${t("compose.format.clear")}</button>
        </div>
        <div class="compose-attachments" id="compose-attachments"></div>
        <input id="compose-file-input" type="file" multiple hidden>
        <div class="modal-footer">
          <div class="modal-tools">
            <button class="modal-tool" id="compose-attach-btn" title="${t("compose.tool.attach")}">
              ${icon("paperclip")}
            </button>
            <button class="modal-tool" id="compose-format-btn" title="${t("compose.tool.format")}">
              ${icon("forms")}
            </button>
          </div>
          <div style="display:flex; gap:8px;">
            <button class="verdant-btn" id="compose-save-draft-btn">${t("compose.save_draft")}</button>
            <button class="send-btn" id="compose-send-btn">
              ${icon("send")}
              ${t("compose.send")}
            </button>
          </div>
        </div>
      </div>
    </div>
  `;
}
