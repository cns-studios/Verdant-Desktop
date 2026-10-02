mod actions;
mod append;
mod attachments;
mod connection;
mod folders;
mod parse;
mod sync;

pub use actions::*;
pub use append::*;
pub use attachments::*;
pub use connection::*;
pub use folders::*;
pub use parse::*;
pub use sync::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, special: Option<&'static str>) -> Folder {
        Folder { name: name.to_string(), special, selectable: true }
    }

    #[test]
    fn decodes_modified_utf7_folder_names() {
        assert_eq!(decode_imap_utf7("Entw&APw-rfe"), "Entwürfe");
        assert_eq!(decode_imap_utf7("Gel&APY-schte Elemente"), "Gelöschte Elemente");
        assert_eq!(decode_imap_utf7("&AMk-l&AOk-ments envoy&AOk-s"), "Éléments envoyés");
        assert_eq!(decode_imap_utf7("Tom &- Jerry"), "Tom & Jerry");
        assert_eq!(decode_imap_utf7("INBOX"), "INBOX");
    }

    #[test]
    fn special_use_flags_win_over_localised_names() {
        let folders = vec![
            folder("INBOX", None),
            folder("&AMk-l&AOk-ments envoy&AOk-s", Some("SENT")),
            folder("Corbeille", Some("TRASH")),
        ];
        assert_eq!(imap_folder_for_mailbox("SENT", &folders).as_deref(), Some("&AMk-l&AOk-ments envoy&AOk-s"));
        assert_eq!(imap_folder_for_mailbox("TRASH", &folders).as_deref(), Some("Corbeille"));
        assert_eq!(imap_folder_for_mailbox("inbox", &folders).as_deref(), Some("INBOX"));
    }

    #[test]
    fn falls_back_to_known_names_and_skips_unselectable() {
        let mut parent = folder("[Gmail]", None);
        parent.selectable = false;
        let folders = vec![parent, folder("[Gmail]/All Mail", None), folder("Entw&APw-rfe", None)];
        assert_eq!(imap_folder_for_mailbox("ARCHIVE", &folders).as_deref(), Some("[Gmail]/All Mail"));
        assert_eq!(imap_folder_for_mailbox("DRAFT", &folders).as_deref(), Some("Entw&APw-rfe"));
        assert_eq!(imap_folder_for_mailbox("SENT", &folders), None);
    }

    #[test]
    fn email_ids_are_stable() {
        assert_eq!(imap_email_id(3, Some("<abc@host>"), "INBOX", "7"), "3:abc@host");
        assert_eq!(imap_email_id(3, None, "SENT", "7"), "3:imap-3-SENT-7");
    }

    #[test]
    fn search_strings_are_quoted() {
        assert_eq!(quote_imap_string(r#"say "hi" \ bye"#), r#""say \"hi\" \\ bye""#);
    }
}
