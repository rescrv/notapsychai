use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_NULL",
        description: "null operation",
    },
    OpString {
        name: "OP_BOTTOM_PAGE",
        description: "move to the bottom of the page",
    },
    OpString {
        name: "OP_BOUNCE_MESSAGE",
        description: "remail a message to another user",
    },
    OpString {
        name: "OP_CATCHUP",
        description: "mark all articles in newsgroup as read",
    },
    OpString {
        name: "OP_CHANGE_DIRECTORY",
        description: "change directories",
    },
    OpString {
        name: "OP_CHECK_NEW",
        description: "check mailboxes for new mail",
    },
    OpString {
        name: "OP_CHECK_STATS",
        description: "calculate message statistics for all mailboxes",
    },
    OpString {
        name: "OP_COPY_MESSAGE",
        description: "copy a message to a file/mailbox",
    },
    OpString {
        name: "OP_CREATE_ALIAS",
        description: "create an alias from a message sender",
    },
    OpString {
        name: "OP_CREATE_MAILBOX",
        description: "create a new mailbox (IMAP only)",
    },
    OpString {
        name: "OP_CURRENT_BOTTOM",
        description: "move entry to bottom of screen",
    },
    OpString {
        name: "OP_CURRENT_MIDDLE",
        description: "move entry to middle of screen",
    },
    OpString {
        name: "OP_CURRENT_TOP",
        description: "move entry to top of screen",
    },
    OpString {
        name: "OP_DECODE_COPY",
        description: "make decoded (text/plain) copy",
    },
    OpString {
        name: "OP_DECODE_SAVE",
        description: "make decoded copy (text/plain) and delete",
    },
    OpString {
        name: "OP_DELETE",
        description: "delete the current entry",
    },
    OpString {
        name: "OP_DELETE_MAILBOX",
        description: "delete the current mailbox (IMAP only)",
    },
    OpString {
        name: "OP_DELETE_SUBTHREAD",
        description: "delete all messages in subthread",
    },
    OpString {
        name: "OP_DELETE_THREAD",
        description: "delete all messages in thread",
    },
    OpString {
        name: "OP_DESCEND_DIRECTORY",
        description: "descend into a directory",
    },
    OpString {
        name: "OP_DISPLAY_ADDRESS",
        description: "display full address of sender",
    },
    OpString {
        name: "OP_DISPLAY_HEADERS",
        description: "display message and toggle header weeding",
    },
    OpString {
        name: "OP_DISPLAY_MESSAGE",
        description: "display a message",
    },
    OpString {
        name: "OP_EDIT_LABEL",
        description: "add, change, or delete a message's label",
    },
    OpString {
        name: "OP_EDIT_OR_VIEW_RAW_MESSAGE",
        description: "edit the raw message if the mailbox is not read-only, otherwise view it",
    },
    OpString {
        name: "OP_EDIT_RAW_MESSAGE",
        description: "edit the raw message (edit and edit-raw-message are synonyms)",
    },
    OpString {
        name: "OP_END_COND",
        description: "end of conditional execution (noop)",
    },
    OpString {
        name: "OP_ENTER_COMMAND",
        description: "enter a neomuttrc command",
    },
    OpString {
        name: "OP_ENTER_MASK",
        description: "enter a file mask",
    },
    OpString {
        name: "OP_EXIT",
        description: "exit this menu",
    },
    OpString {
        name: "OP_FIRST_ENTRY",
        description: "move to the first entry",
    },
    OpString {
        name: "OP_FLAG_MESSAGE",
        description: "toggle a message's 'important' flag",
    },
    OpString {
        name: "OP_FOLLOWUP",
        description: "followup to newsgroup",
    },
    OpString {
        name: "OP_FORWARD_MESSAGE",
        description: "forward a message with comments",
    },
    OpString {
        name: "OP_FORWARD_TO_GROUP",
        description: "forward to newsgroup",
    },
    OpString {
        name: "OP_GENERIC_SELECT_ENTRY",
        description: "select the current entry",
    },
    OpString {
        name: "OP_GET_CHILDREN",
        description: "get all children of the current message",
    },
    OpString {
        name: "OP_GET_MESSAGE",
        description: "get message with Message-Id",
    },
    OpString {
        name: "OP_GET_PARENT",
        description: "get parent of the current message",
    },
    OpString {
        name: "OP_GOTO_PARENT",
        description: "go to parent directory",
    },
    OpString {
        name: "OP_GROUP_CHAT_REPLY",
        description: "reply to all recipients preserving To/Cc",
    },
    OpString {
        name: "OP_GROUP_REPLY",
        description: "reply to all recipients",
    },
    OpString {
        name: "OP_HALF_DOWN",
        description: "scroll down 1/2 page",
    },
    OpString {
        name: "OP_HALF_UP",
        description: "scroll up 1/2 page",
    },
    OpString {
        name: "OP_HELP",
        description: "this screen",
    },
    OpString {
        name: "OP_JUMP",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_1",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_2",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_3",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_4",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_5",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_6",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_7",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_8",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_JUMP_9",
        description: "jump to an index number",
    },
    OpString {
        name: "OP_LAST_ENTRY",
        description: "move to the last entry",
    },
    OpString {
        name: "OP_LIMIT_CURRENT_THREAD",
        description: "limit view to current thread",
    },
    OpString {
        name: "OP_LIST_REPLY",
        description: "reply to specified mailing list",
    },
    OpString {
        name: "OP_LIST_SUBSCRIBE",
        description: "subscribe to a mailing list",
    },
    OpString {
        name: "OP_LIST_UNSUBSCRIBE",
        description: "unsubscribe from a mailing list",
    },
    OpString {
        name: "OP_LOAD_ACTIVE",
        description: "load list of all newsgroups from NNTP server",
    },
    OpString {
        name: "OP_MACRO",
        description: "execute a macro",
    },
    OpString {
        name: "OP_MAIL",
        description: "compose a new mail message",
    },
    OpString {
        name: "OP_MAILBOX_LIST",
        description: "list mailboxes with new mail",
    },
    OpString {
        name: "OP_MAIN_BREAK_THREAD",
        description: "break the thread in two",
    },
    OpString {
        name: "OP_MAIN_CHANGE_FOLDER",
        description: "open a different folder",
    },
    OpString {
        name: "OP_MAIN_CHANGE_FOLDER_READONLY",
        description: "open a different folder in read only mode",
    },
    OpString {
        name: "OP_MAIN_CHANGE_GROUP",
        description: "open a different newsgroup",
    },
    OpString {
        name: "OP_MAIN_CHANGE_GROUP_READONLY",
        description: "open a different newsgroup in read only mode",
    },
    OpString {
        name: "OP_MAIN_CLEAR_FLAG",
        description: "clear a status flag from a message",
    },
    OpString {
        name: "OP_MAIN_COLLAPSE_ALL",
        description: "collapse/uncollapse all threads",
    },
    OpString {
        name: "OP_MAIN_COLLAPSE_THREAD",
        description: "collapse/uncollapse current thread",
    },
    OpString {
        name: "OP_MAIN_DELETE_PATTERN",
        description: "delete non-hidden messages matching a pattern",
    },
    OpString {
        name: "OP_MAIN_FETCH_MAIL",
        description: "retrieve mail from POP server",
    },
    OpString {
        name: "OP_MAIN_IMAP_FETCH",
        description: "force retrieval of mail from IMAP server",
    },
    OpString {
        name: "OP_MAIN_IMAP_LOGOUT_ALL",
        description: "logout from all IMAP servers",
    },
    OpString {
        name: "OP_MAIN_LIMIT",
        description: "show only messages matching a pattern",
    },
    OpString {
        name: "OP_MAIN_LINK_THREADS",
        description: "link tagged message to the current one",
    },
    OpString {
        name: "OP_MAIN_MODIFY_TAGS",
        description: "modify (notmuch/imap) tags",
    },
    OpString {
        name: "OP_MAIN_MODIFY_TAGS_THEN_HIDE",
        description: "modify (notmuch/imap) tags and then hide message",
    },
    OpString {
        name: "OP_MAIN_NEXT_NEW",
        description: "jump to the next new message",
    },
    OpString {
        name: "OP_MAIN_NEXT_NEW_THEN_UNREAD",
        description: "jump to the next new or unread message",
    },
    OpString {
        name: "OP_MAIN_NEXT_SUBTHREAD",
        description: "jump to the next subthread",
    },
    OpString {
        name: "OP_MAIN_NEXT_THREAD",
        description: "jump to the next thread",
    },
    OpString {
        name: "OP_MAIN_NEXT_UNDELETED",
        description: "move to the next undeleted message",
    },
    OpString {
        name: "OP_MAIN_NEXT_UNREAD",
        description: "jump to the next unread message",
    },
    OpString {
        name: "OP_MAIN_NEXT_UNREAD_MAILBOX",
        description: "open next mailbox with new mail",
    },
    OpString {
        name: "OP_MAIN_PARENT_MESSAGE",
        description: "jump to parent message in thread",
    },
    OpString {
        name: "OP_MAIN_PREV_NEW",
        description: "jump to the previous new message",
    },
    OpString {
        name: "OP_MAIN_PREV_NEW_THEN_UNREAD",
        description: "jump to the previous new or unread message",
    },
    OpString {
        name: "OP_MAIN_PREV_SUBTHREAD",
        description: "jump to previous subthread",
    },
    OpString {
        name: "OP_MAIN_PREV_THREAD",
        description: "jump to previous thread",
    },
    OpString {
        name: "OP_MAIN_PREV_UNDELETED",
        description: "move to the previous undeleted message",
    },
    OpString {
        name: "OP_MAIN_PREV_UNREAD",
        description: "jump to the previous unread message",
    },
    OpString {
        name: "OP_MAIN_QUASI_DELETE",
        description: "delete from NeoMutt, don't touch on disk",
    },
    OpString {
        name: "OP_MAIN_READ_SUBTHREAD",
        description: "mark the current subthread as read",
    },
    OpString {
        name: "OP_MAIN_READ_THREAD",
        description: "mark the current thread as read",
    },
    OpString {
        name: "OP_MAIN_ROOT_MESSAGE",
        description: "jump to root message in thread",
    },
    OpString {
        name: "OP_MAIN_SET_FLAG",
        description: "set a status flag on a message",
    },
    OpString {
        name: "OP_MAIN_SHOW_LIMIT",
        description: "show currently active limit pattern",
    },
    OpString {
        name: "OP_MAIN_SYNC_FOLDER",
        description: "save changes to mailbox",
    },
    OpString {
        name: "OP_MAIN_TAG_PATTERN",
        description: "tag non-hidden messages matching a pattern",
    },
    OpString {
        name: "OP_MAIN_UNDELETE_PATTERN",
        description: "undelete non-hidden messages matching a pattern",
    },
    OpString {
        name: "OP_MAIN_UNTAG_PATTERN",
        description: "untag non-hidden messages matching a pattern",
    },
    OpString {
        name: "OP_MARK_MSG",
        description: "create a hotkey macro for the current message",
    },
    OpString {
        name: "OP_MIDDLE_PAGE",
        description: "move to the middle of the page",
    },
    OpString {
        name: "OP_NEXT_ENTRY",
        description: "move to the next entry",
    },
    OpString {
        name: "OP_NEXT_LINE",
        description: "scroll down one line",
    },
    OpString {
        name: "OP_NEXT_PAGE",
        description: "move to the next page",
    },
    OpString {
        name: "OP_PIPE",
        description: "pipe message/attachment to a shell command",
    },
    OpString {
        name: "OP_POST",
        description: "post message to newsgroup",
    },
    OpString {
        name: "OP_PREV_ENTRY",
        description: "move to the previous entry",
    },
    OpString {
        name: "OP_PREV_LINE",
        description: "scroll up one line",
    },
    OpString {
        name: "OP_PREV_PAGE",
        description: "move to the previous page",
    },
    OpString {
        name: "OP_PRINT",
        description: "print the current entry",
    },
    OpString {
        name: "OP_PURGE_MESSAGE",
        description: "delete the current entry, bypassing the trash folder",
    },
    OpString {
        name: "OP_PURGE_THREAD",
        description: "delete the current thread, bypassing the trash folder",
    },
    OpString {
        name: "OP_QUERY",
        description: "query external program for addresses",
    },
    OpString {
        name: "OP_QUIT",
        description: "save changes to mailbox and quit",
    },
    OpString {
        name: "OP_RECALL_MESSAGE",
        description: "recall a postponed message",
    },
    OpString {
        name: "OP_RECONSTRUCT_THREAD",
        description: "reconstruct thread containing current message",
    },
    OpString {
        name: "OP_REDRAW",
        description: "clear and redraw the screen",
    },
    OpString {
        name: "OP_RENAME_MAILBOX",
        description: "rename the current mailbox (IMAP only)",
    },
    OpString {
        name: "OP_REPLY",
        description: "reply to a message",
    },
    OpString {
        name: "OP_RESEND",
        description: "use the current message as a template for a new one",
    },
    OpString {
        name: "OP_SAVE",
        description: "save message/attachment to a mailbox/file",
    },
    OpString {
        name: "OP_SEARCH",
        description: "search for a regular expression",
    },
    OpString {
        name: "OP_SEARCH_NEXT",
        description: "search for next match",
    },
    OpString {
        name: "OP_SEARCH_OPPOSITE",
        description: "search for next match in opposite direction",
    },
    OpString {
        name: "OP_SEARCH_REVERSE",
        description: "search backwards for a regular expression",
    },
    OpString {
        name: "OP_SEARCH_TOGGLE",
        description: "toggle search pattern coloring",
    },
    OpString {
        name: "OP_SHELL_ESCAPE",
        description: "invoke a command in a subshell",
    },
    OpString {
        name: "OP_SHOW_LOG_MESSAGES",
        description: "show log (and debug) messages",
    },
    OpString {
        name: "OP_SORT",
        description: "sort messages",
    },
    OpString {
        name: "OP_SORT_REVERSE",
        description: "sort messages in reverse order",
    },
    OpString {
        name: "OP_SUBSCRIBE_PATTERN",
        description: "subscribe to newsgroups matching a pattern",
    },
    OpString {
        name: "OP_TAG",
        description: "tag the current entry",
    },
    OpString {
        name: "OP_TAG_PREFIX",
        description: "apply next function to tagged messages",
    },
    OpString {
        name: "OP_TAG_PREFIX_COND",
        description: "apply next function ONLY to tagged messages",
    },
    OpString {
        name: "OP_TAG_SUBTHREAD",
        description: "tag the current subthread",
    },
    OpString {
        name: "OP_TAG_THREAD",
        description: "tag the current thread",
    },
    OpString {
        name: "OP_TOGGLE_MAILBOXES",
        description: "toggle whether to browse mailboxes or all files",
    },
    OpString {
        name: "OP_TOGGLE_NEW",
        description: "toggle a message's 'new' flag",
    },
    OpString {
        name: "OP_TOGGLE_READ",
        description: "toggle view of read messages",
    },
    OpString {
        name: "OP_TOGGLE_WRITE",
        description: "toggle whether the mailbox will be rewritten",
    },
    OpString {
        name: "OP_TOP_PAGE",
        description: "move to the top of the page",
    },
    OpString {
        name: "OP_UNCATCHUP",
        description: "mark all articles in newsgroup as unread",
    },
    OpString {
        name: "OP_UNDELETE",
        description: "undelete the current entry",
    },
    OpString {
        name: "OP_UNDELETE_SUBTHREAD",
        description: "undelete all messages in subthread",
    },
    OpString {
        name: "OP_UNDELETE_THREAD",
        description: "undelete all messages in thread",
    },
    OpString {
        name: "OP_UNSUBSCRIBE_PATTERN",
        description: "unsubscribe from newsgroups matching a pattern",
    },
    OpString {
        name: "OP_VERSION",
        description: "show the NeoMutt version number and date",
    },
    OpString {
        name: "OP_VIEW_ATTACHMENTS",
        description: "show MIME attachments",
    },
    OpString {
        name: "OP_VIEW_RAW_MESSAGE",
        description: "show the raw message",
    },
    OpString {
        name: "OP_WHAT_KEY",
        description: "display the keycode for a key press",
    },
    OpString {
        name: "OP_DECRYPT_COPY",
        description: "make decrypted copy",
    },
    OpString {
        name: "OP_DECRYPT_SAVE",
        description: "make decrypted copy and delete",
    },
    OpString {
        name: "OP_EXTRACT_KEYS",
        description: "extract supported public keys",
    },
    OpString {
        name: "OP_FORGET_PASSPHRASE",
        description: "wipe passphrases from memory",
    },
    OpString {
        name: "OP_CHECK_TRADITIONAL",
        description: "check for classic PGP",
    },
    OpString {
        name: "OP_MAIL_KEY",
        description: "mail a PGP public key",
    },
    OpString {
        name: "OP_VERIFY_KEY",
        description: "verify a public key",
    },
    OpString {
        name: "OP_VIEW_ID",
        description: "view the key's user id",
    },
];
