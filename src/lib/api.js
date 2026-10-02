import { invoke } from "@tauri-apps/api/core";

export const authStatus = () => invoke("auth_status");
export const getUserProfile = () => invoke("get_user_profile");

export const listAccounts = () => invoke("list_accounts");
export const switchAccount = (accountId) => invoke("switch_account", { accountId });
export const removeAccount = (accountId) => invoke("remove_account", { accountId });
export const addGmailAccount = () => invoke("add_gmail_account");
export const addImapAccount = (payload) => invoke("add_imap_account", { payload });
export const testImapCredentials = (payload) => invoke("test_imap_credentials", { payload });
export const getActiveAccountInfo = () => invoke("get_active_account_info");

export const syncEmails = () => invoke("sync_emails");
export const syncMailbox = (mailbox) => invoke("sync_mailbox", { mailbox });
export const syncMailboxPage = (mailbox, pageToken) => invoke("sync_mailbox_page", { mailbox, pageToken });
export const syncImapMailboxPage = (mailbox, offset) => invoke("sync_imap_mailbox_page", { mailbox, offset });
export const getEmails = (mailbox) => invoke("get_emails", { mailbox });
export const getEmail = (emailId) => invoke("get_email", { emailId });
export const deepSearchEmails = (query) => invoke("deep_search_emails", { query });
export const getMailboxCounts = () => invoke("get_mailbox_counts");
export const clearLocalData = () => invoke("clear_local_data");

export const getInboxThreads = () => invoke("get_inbox_threads");
export const getThreadMessages = (threadId) => invoke("get_thread_messages", { threadId });
export const markThreadRead = (threadId) => invoke("mark_thread_read", { threadId });
export const markEmailsRead = (emailIds) => invoke("mark_emails_read", { emailIds });

export const setEmailReadStatus = (emailId, isRead) => invoke("set_email_read_status", { emailId, isRead });
export const toggleStarred = (emailId) => invoke("toggle_starred", { emailId });
export const archiveEmail = (emailId) => invoke("archive_email", { emailId });
export const trashEmail = (emailId) => invoke("trash_email", { emailId });
export const permanentDeleteEmail = (emailId) => invoke("permanent_delete_email", { emailId });
export const restoreFromTrash = (emailId) => invoke("restore_from_trash", { emailId });
export const moveToInbox = (emailId) => invoke("move_to_inbox", { emailId });
export const unsubscribeFromList = (emailId) => invoke("unsubscribe_from_list", { emailId });

export const sendEmail = (payload) => invoke("send_email", payload);
export const saveDraft = (payload) => invoke("save_draft", payload);
export const sendExistingDraft = (draftId) => invoke("send_existing_draft", { draftId });
export const downloadAttachment = (emailId, attachmentId, filename, contentType) =>
  invoke("download_attachment", { emailId, attachmentId, filename, contentType });

export const getInboxCategories = () => invoke("get_inbox_categories");
export const getSmartInboxEnabled = () => invoke("get_smart_inbox_enabled");
export const setSmartInboxEnabled = (enabled) => invoke("set_smart_inbox_enabled", { enabled });
export const previewInboxCategories = () => invoke("preview_inbox_categories");
export const categorizeInbox = () => invoke("categorize_inbox");
export const abortCategorizeInbox = () => invoke("abort_categorize_inbox");
export const getCategorizeProgress = () => invoke("get_categorize_progress");
export const renameInboxCategory = (slug, name) => invoke("rename_inbox_category", { slug, name });
export const getCategoryThreads = (slug) => invoke("get_category_threads", { slug });
export const moveEmailsToCategory = (emailIds, slug) => invoke("move_emails_to_category", { emailIds, slug });

export const getAppConfig = () => invoke("get_app_config");
export const updateAppConfig = (config) => invoke("update_app_config", { config });
export const autostartEnable = () => invoke("autostart_enable");
export const autostartDisable = () => invoke("autostart_disable");
export const getStartupFlags = () => invoke("get_startup_flags");
export const hideMainWindow = () => invoke("hide_main_window");

export const checkForUpdates = (channel) => invoke("check_for_updates", { channel });
export const downloadLatestUpdate = (channel) => invoke("download_latest_update", { channel });
export const installAndRelaunch = (filePath) => invoke("install_and_relaunch", { filePath });
export const getChangelog = (version) => invoke("get_changelog", { version });

export const openExternalUrl = (url) => invoke("open_external_url", { url });
export const fetchRemoteImage = (url) => invoke("fetch_remote_image", { url });
