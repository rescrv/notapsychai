//! GUI opcode catalog mirrored from neomutt/gui/opcodes.h.

/// Operation codes understood by the GUI layer.
///
/// Source: neomutt/gui/opcodes.h (OPS_* macro groups, excluding AUTOCRYPT/NOTMUCH).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum OpCode {
    /// Repaint is needed.
    Repaint = -3,
    /// Timeout event with no input.
    Timeout = -2,
    /// Abort key pressed (Ctrl-G).
    Abort = -1,
    /// Null operation.
    Null,
    /// Attach files to this message.
    AttachmentAttachFile,
    /// Attach messages to this message.
    AttachmentAttachMessage,
    /// Attach news articles to this message.
    AttachmentAttachNewsMessage,
    /// Toggle display of subparts.
    AttachmentCollapse,
    /// Delete the current entry.
    AttachmentDelete,
    /// Detach the current entry.
    AttachmentDetach,
    /// Edit the 'Content-ID' of the attachment.
    AttachmentEditContentId,
    /// Edit attachment description.
    AttachmentEditDescription,
    /// Edit attachment transfer-encoding.
    AttachmentEditEncoding,
    /// Edit the 'Content-Language' of the attachment.
    AttachmentEditLanguage,
    /// Edit attachment using mailcap entry.
    AttachmentEditMime,
    /// Edit attachment content type.
    AttachmentEditType,
    /// Filter attachment through a shell command.
    AttachmentFilter,
    /// Get a temporary copy of an attachment.
    AttachmentGetAttachment,
    /// Group tagged attachments as 'multipart/alternative'.
    AttachmentGroupAlts,
    /// Group tagged attachments as 'multipart/multilingual'.
    AttachmentGroupLingual,
    /// Group tagged attachments as 'multipart/related'.
    AttachmentGroupRelated,
    /// Move an attachment down in the attachment list.
    AttachmentMoveDown,
    /// Move an attachment up in the attachment list.
    AttachmentMoveUp,
    /// Compose new attachment using mailcap entry.
    AttachmentNewMime,
    /// Pipe message/attachment to a shell command.
    AttachmentPipe,
    /// Print the current entry.
    AttachmentPrint,
    /// Send attachment with a different name.
    AttachmentRenameAttachment,
    /// Save message/attachment to a mailbox/file.
    AttachmentSave,
    /// Toggle disposition between inline/attachment.
    AttachmentToggleDisposition,
    /// Toggle recoding of this attachment.
    AttachmentToggleRecode,
    /// Toggle whether to delete file after sending it.
    AttachmentToggleUnlink,
    /// Undelete the current entry.
    AttachmentUndelete,
    /// Ungroup 'multipart' attachment.
    AttachmentUngroup,
    /// Update an attachment's encoding info.
    AttachmentUpdateEncoding,
    /// View attachment using mailcap entry if necessary.
    AttachmentView,
    /// Force viewing of attachment using mailcap.
    AttachmentViewMailcap,
    /// View attachment in pager using copiousoutput mailcap.
    AttachmentViewPager,
    /// View attachment as text.
    AttachmentViewText,
    /// Show the next page of the message.
    PreviewPageDown,
    /// Show the previous page of the message.
    PreviewPageUp,
    /// Open the aliases dialog.
    AliasDialog,
    /// Move to the bottom of the page.
    BottomPage,
    /// Remail a message to another user.
    BounceMessage,
    /// Swap the current folder position with $folder if it exists.
    BrowserGotoFolder,
    /// Select a new file in this directory.
    BrowserNewFile,
    /// Subscribe to current mbox (IMAP/NNTP only).
    BrowserSubscribe,
    /// Display the currently selected file's name.
    BrowserTell,
    /// Toggle view all/subscribed mailboxes (IMAP only).
    BrowserToggleLsub,
    /// Unsubscribe from current mbox (IMAP/NNTP only).
    BrowserUnsubscribe,
    /// View file.
    BrowserViewFile,
    /// Mark all articles in newsgroup as read.
    Catchup,
    /// Change directories.
    ChangeDirectory,
    /// Check mailboxes for new mail.
    CheckNew,
    /// Calculate message statistics for all mailboxes.
    CheckStats,
    /// Edit the file to be attached.
    ComposeEditFile,
    /// Edit the message.
    ComposeEditMessage,
    /// Run ispell on the message.
    ComposeIspell,
    /// Save this message to send later.
    ComposePostponeMessage,
    /// Rename/move an attached file.
    ComposeRenameFile,
    /// Send the message.
    ComposeSendMessage,
    /// Compose new message to the current message sender.
    ComposeToSender,
    /// Write the message to a folder.
    ComposeWriteMessage,
    /// Copy a message to a file/mailbox.
    CopyMessage,
    /// Create an alias from a message sender.
    CreateAlias,
    /// Create a new mailbox (IMAP only).
    CreateMailbox,
    /// Move entry to bottom of screen.
    CurrentBottom,
    /// Move entry to middle of screen.
    CurrentMiddle,
    /// Move entry to top of screen.
    CurrentTop,
    /// Make decoded (text/plain) copy.
    DecodeCopy,
    /// Make decoded copy (text/plain) and delete.
    DecodeSave,
    /// Delete the current entry.
    Delete,
    /// Delete the current mailbox (IMAP only).
    DeleteMailbox,
    /// Delete all messages in subthread.
    DeleteSubthread,
    /// Delete all messages in thread.
    DeleteThread,
    /// Descend into a directory.
    DescendDirectory,
    /// Display full address of sender.
    DisplayAddress,
    /// Display message and toggle header weeding.
    DisplayHeaders,
    /// Display a message.
    DisplayMessage,
    /// Delete the char in front of the cursor.
    EditorBackspace,
    /// Move the cursor one character to the left.
    EditorBackwardChar,
    /// Move the cursor to the beginning of the word.
    EditorBackwardWord,
    /// Jump to the beginning of the line.
    EditorBol,
    /// Capitalize the word.
    EditorCapitalizeWord,
    /// Complete filename or alias.
    EditorComplete,
    /// Complete address with query.
    EditorCompleteQuery,
    /// Delete the char under the cursor.
    EditorDeleteChar,
    /// Convert the word to lower case.
    EditorDowncaseWord,
    /// Jump to the end of the line.
    EditorEol,
    /// Move the cursor one character to the right.
    EditorForwardChar,
    /// Move the cursor to the end of the word.
    EditorForwardWord,
    /// Scroll down through the history list.
    EditorHistoryDown,
    /// Search through the history list.
    EditorHistorySearch,
    /// Scroll up through the history list.
    EditorHistoryUp,
    /// Delete chars from cursor to end of line.
    EditorKillEol,
    /// Delete chars from the cursor to the end of the word.
    EditorKillEow,
    /// Delete chars from cursor to beginning the line.
    EditorKillLine,
    /// Delete all chars on the line.
    EditorKillWholeLine,
    /// Delete the word in front of the cursor.
    EditorKillWord,
    /// Cycle among incoming mailboxes.
    EditorMailboxCycle,
    /// Quote the next typed key.
    EditorQuoteChar,
    /// Transpose character under cursor with previous.
    EditorTransposeChars,
    /// Convert the word to upper case.
    EditorUpcaseWord,
    /// Add, change, or delete a message's label.
    EditLabel,
    /// Edit the raw message if the mailbox is not read-only, otherwise view it.
    EditOrViewRawMessage,
    /// Edit the raw message (edit and edit-raw-message are synonyms).
    EditRawMessage,
    /// End of conditional execution (noop).
    EndCond,
    /// Enter a neomuttrc command.
    EnterCommand,
    /// Enter a file mask.
    EnterMask,
    /// Exit this menu.
    Exit,
    /// Move to the first entry.
    FirstEntry,
    /// Toggle a message's 'important' flag.
    FlagMessage,
    /// Followup to newsgroup.
    Followup,
    /// Forward a message with comments.
    ForwardMessage,
    /// Forward to newsgroup.
    ForwardToGroup,
    /// Select the current entry.
    GenericSelectEntry,
    /// Get all children of the current message.
    GetChildren,
    /// Get message with Message-Id.
    GetMessage,
    /// Get parent of the current message.
    GetParent,
    /// Go to parent directory.
    GotoParent,
    /// Reply to all recipients preserving To/Cc.
    GroupChatReply,
    /// Reply to all recipients.
    GroupReply,
    /// Scroll down 1/2 page.
    HalfDown,
    /// Scroll up 1/2 page.
    HalfUp,
    /// This screen.
    Help,
    /// Jump to an index number.
    Jump,
    /// Jump to an index number.
    Jump1,
    /// Jump to an index number.
    Jump2,
    /// Jump to an index number.
    Jump3,
    /// Jump to an index number.
    Jump4,
    /// Jump to an index number.
    Jump5,
    /// Jump to an index number.
    Jump6,
    /// Jump to an index number.
    Jump7,
    /// Jump to an index number.
    Jump8,
    /// Jump to an index number.
    Jump9,
    /// Move to the last entry.
    LastEntry,
    /// Limit view to current thread.
    LimitCurrentThread,
    /// Reply to specified mailing list.
    ListReply,
    /// Subscribe to a mailing list.
    ListSubscribe,
    /// Unsubscribe from a mailing list.
    ListUnsubscribe,
    /// Load list of all newsgroups from NNTP server.
    LoadActive,
    /// Execute a macro.
    Macro,
    /// Compose a new mail message.
    Mail,
    /// List mailboxes with new mail.
    MailboxList,
    /// Break the thread in two.
    MainBreakThread,
    /// Open a different folder.
    MainChangeFolder,
    /// Open a different folder in read only mode.
    MainChangeFolderReadonly,
    /// Open a different newsgroup.
    MainChangeGroup,
    /// Open a different newsgroup in read only mode.
    MainChangeGroupReadonly,
    /// Clear a status flag from a message.
    MainClearFlag,
    /// Collapse/uncollapse all threads.
    MainCollapseAll,
    /// Collapse/uncollapse current thread.
    MainCollapseThread,
    /// Delete non-hidden messages matching a pattern.
    MainDeletePattern,
    /// Retrieve mail from POP server.
    MainFetchMail,
    /// Force retrieval of mail from IMAP server.
    MainImapFetch,
    /// Logout from all IMAP servers.
    MainImapLogoutAll,
    /// Show only messages matching a pattern.
    MainLimit,
    /// Link tagged message to the current one.
    MainLinkThreads,
    /// Modify (notmuch/imap) tags.
    MainModifyTags,
    /// Modify (notmuch/imap) tags and then hide message.
    MainModifyTagsThenHide,
    /// Jump to the next new message.
    MainNextNew,
    /// Jump to the next new or unread message.
    MainNextNewThenUnread,
    /// Jump to the next subthread.
    MainNextSubthread,
    /// Jump to the next thread.
    MainNextThread,
    /// Move to the next undeleted message.
    MainNextUndeleted,
    /// Jump to the next unread message.
    MainNextUnread,
    /// Open next mailbox with new mail.
    MainNextUnreadMailbox,
    /// Jump to parent message in thread.
    MainParentMessage,
    /// Jump to the previous new message.
    MainPrevNew,
    /// Jump to the previous new or unread message.
    MainPrevNewThenUnread,
    /// Jump to previous subthread.
    MainPrevSubthread,
    /// Jump to previous thread.
    MainPrevThread,
    /// Move to the previous undeleted message.
    MainPrevUndeleted,
    /// Jump to the previous unread message.
    MainPrevUnread,
    /// Delete from NeoMutt, don't touch on disk.
    MainQuasiDelete,
    /// Mark the current subthread as read.
    MainReadSubthread,
    /// Mark the current thread as read.
    MainReadThread,
    /// Jump to root message in thread.
    MainRootMessage,
    /// Set a status flag on a message.
    MainSetFlag,
    /// Show currently active limit pattern.
    MainShowLimit,
    /// Save changes to mailbox.
    MainSyncFolder,
    /// Tag non-hidden messages matching a pattern.
    MainTagPattern,
    /// Undelete non-hidden messages matching a pattern.
    MainUndeletePattern,
    /// Untag non-hidden messages matching a pattern.
    MainUntagPattern,
    /// Create a hotkey macro for the current message.
    MarkMsg,
    /// Move to the middle of the page.
    MiddlePage,
    /// Move to the next entry.
    NextEntry,
    /// Scroll down one line.
    NextLine,
    /// Move to the next page.
    NextPage,
    /// Jump to the bottom of the message.
    PagerBottom,
    /// Toggle display of quoted text.
    PagerHideQuoted,
    /// Jump to first line after headers.
    PagerSkipHeaders,
    /// Skip beyond quoted text.
    PagerSkipQuoted,
    /// Jump to the top of the message.
    PagerTop,
    /// Pipe message/attachment to a shell command.
    Pipe,
    /// Post message to newsgroup.
    Post,
    /// Move to the previous entry.
    PrevEntry,
    /// Scroll up one line.
    PrevLine,
    /// Move to the previous page.
    PrevPage,
    /// Print the current entry.
    Print,
    /// Delete the current entry, bypassing the trash folder.
    PurgeMessage,
    /// Delete the current thread, bypassing the trash folder.
    PurgeThread,
    /// Query external program for addresses.
    Query,
    /// Append new query results to current results.
    QueryAppend,
    /// Save changes to mailbox and quit.
    Quit,
    /// Recall a postponed message.
    RecallMessage,
    /// Reconstruct thread containing current message.
    ReconstructThread,
    /// Clear and redraw the screen.
    Redraw,
    /// Rename the current mailbox (IMAP only).
    RenameMailbox,
    /// Reply to a message.
    Reply,
    /// Use the current message as a template for a new one.
    Resend,
    /// Save message/attachment to a mailbox/file.
    Save,
    /// Search for a regular expression.
    Search,
    /// Search for next match.
    SearchNext,
    /// Search for next match in opposite direction.
    SearchOpposite,
    /// Search backwards for a regular expression.
    SearchReverse,
    /// Toggle search pattern coloring.
    SearchToggle,
    /// Invoke a command in a subshell.
    ShellEscape,
    /// Show log (and debug) messages.
    ShowLogMessages,
    /// Sort messages.
    Sort,
    /// Sort messages in reverse order.
    SortReverse,
    /// Subscribe to newsgroups matching a pattern.
    SubscribePattern,
    /// Tag the current entry.
    Tag,
    /// Apply next function to tagged messages.
    TagPrefix,
    /// Apply next function ONLY to tagged messages.
    TagPrefixCond,
    /// Tag the current subthread.
    TagSubthread,
    /// Tag the current thread.
    TagThread,
    /// Toggle whether to browse mailboxes or all files.
    ToggleMailboxes,
    /// Toggle a message's 'new' flag.
    ToggleNew,
    /// Toggle view of read messages.
    ToggleRead,
    /// Toggle whether the mailbox will be rewritten.
    ToggleWrite,
    /// Move to the top of the page.
    TopPage,
    /// Mark all articles in newsgroup as unread.
    Uncatchup,
    /// Undelete the current entry.
    Undelete,
    /// Undelete all messages in subthread.
    UndeleteSubthread,
    /// Undelete all messages in thread.
    UndeleteThread,
    /// Unsubscribe from newsgroups matching a pattern.
    UnsubscribePattern,
    /// Show the NeoMutt version number and date.
    Version,
    /// Show MIME attachments.
    ViewAttachments,
    /// Show the raw message.
    ViewRawMessage,
    /// Display the keycode for a key press.
    WhatKey,
    /// Make decrypted copy.
    DecryptCopy,
    /// Make decrypted copy and delete.
    DecryptSave,
    /// Extract supported public keys.
    ExtractKeys,
    /// Wipe passphrases from memory.
    ForgetPassphrase,
    /// Edit the BCC list.
    EnvelopeEditBcc,
    /// Edit the CC list.
    EnvelopeEditCc,
    /// Enter a file to save a copy of this message in.
    EnvelopeEditFcc,
    /// Edit the Followup-To field.
    EnvelopeEditFollowupTo,
    /// Edit the from field.
    EnvelopeEditFrom,
    /// Edit the message with headers.
    EnvelopeEditHeaders,
    /// Edit the newsgroups list.
    EnvelopeEditNewsgroups,
    /// Edit the Reply-To field.
    EnvelopeEditReplyTo,
    /// Edit the subject of this message.
    EnvelopeEditSubject,
    /// Edit the TO list.
    EnvelopeEditTo,
    /// Edit the X-Comment-To field.
    EnvelopeEditXCommentTo,
    /// Attach a PGP public key.
    AttachmentAttachKey,
    /// Check for classic PGP.
    CheckTraditional,
    /// Show PGP options.
    ComposePgpMenu,
    /// Mail a PGP public key.
    MailKey,
    /// Verify a public key.
    VerifyKey,
    /// View the key's user id.
    ViewId,
    /// Move the highlight to the first mailbox.
    SidebarFirst,
    /// Move the highlight to the last mailbox.
    SidebarLast,
    /// Move the highlight to next mailbox.
    SidebarNext,
    /// Move the highlight to next mailbox with new mail.
    SidebarNextNew,
    /// Open highlighted mailbox.
    SidebarOpen,
    /// Scroll the sidebar down 1 page.
    SidebarPageDown,
    /// Scroll the sidebar up 1 page.
    SidebarPageUp,
    /// Move the highlight to previous mailbox.
    SidebarPrev,
    /// Move the highlight to previous mailbox with new mail.
    SidebarPrevNew,
    /// Toggle between mailboxes and virtual mailboxes.
    SidebarToggleVirtual,
    /// Make the sidebar (in)visible.
    SidebarToggleVisible,
    /// Show S/MIME options.
    ComposeSmimeMenu,
    /// Sentinel (end of catalog).
    Max,
}

use std::sync::OnceLock;

/// Name/description pair for an opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpString {
    /// Opcode name.
    pub name: &'static str,
    /// Opcode description.
    pub description: &'static str,
}

/// Looks up an opcode entry by name from a slice of opcodes.
fn lookup_in(opcodes: &[OpString], name: &str) -> Option<OpString> {
    opcodes.iter().find(|entry| entry.name == name).copied()
}

mod alias;
mod attachment;
mod browser;
mod compose;
mod editor;
mod envelope;
mod misc;
mod order;
mod pager;
mod preview;
mod query;
mod sidebar;

static OPCODE_STRINGS: OnceLock<Vec<OpString>> = OnceLock::new();

fn opcode_strings() -> &'static [OpString] {
    OPCODE_STRINGS.get_or_init(|| {
        let mut entries = Vec::new();
        for name in order::OPCODE_ORDER {
            if let Some(entry) = lookup_opcode_entry(name) {
                entries.push(entry);
            }
        }
        entries
    })
}

fn lookup_opcode_entry(name: &str) -> Option<OpString> {
    lookup_in(attachment::OPCODES, name)
        .or_else(|| lookup_in(preview::OPCODES, name))
        .or_else(|| lookup_in(alias::OPCODES, name))
        .or_else(|| lookup_in(browser::OPCODES, name))
        .or_else(|| lookup_in(compose::OPCODES, name))
        .or_else(|| lookup_in(editor::OPCODES, name))
        .or_else(|| lookup_in(envelope::OPCODES, name))
        .or_else(|| lookup_in(pager::OPCODES, name))
        .or_else(|| lookup_in(query::OPCODES, name))
        .or_else(|| lookup_in(sidebar::OPCODES, name))
        .or_else(|| lookup_in(misc::OPCODES, name))
}

impl OpCode {
    /// Returns the opcode name.
    pub fn name(self) -> &'static str {
        opcodes_get_name(self as i32)
    }

    /// Returns a human-readable description.
    pub fn description(self) -> &'static str {
        opcodes_get_description(self as i32)
    }
}

/// Returns the name and description for special negative opcodes.
fn special_opcode_strings(op: i32) -> Option<(&'static str, &'static str)> {
    match op {
        _ if op == OpCode::Abort as i32 => Some(("OP_ABORT", "abort the current action")),
        _ if op == OpCode::Timeout as i32 => Some(("OP_TIMEOUT", "timeout occurred")),
        _ if op == OpCode::Repaint as i32 => Some(("OP_REPAINT", "repaint required")),
        _ => None,
    }
}

/// Returns true if the opcode is in the valid range.
fn opcode_in_bounds(op: i32) -> bool {
    op >= OpCode::Repaint as i32 && op < OpCode::Max as i32
}

/// Returns the name for an opcode.
pub fn opcodes_get_name(op: i32) -> &'static str {
    if !opcode_in_bounds(op) {
        return "[UNKNOWN]";
    }

    if let Some((name, _)) = special_opcode_strings(op) {
        return name;
    }

    let idx = op as usize;
    opcode_strings()
        .get(idx)
        .map(|entry| entry.name)
        .unwrap_or("[UNKNOWN]")
}

/// Returns the description for an opcode.
pub fn opcodes_get_description(op: i32) -> &'static str {
    if !opcode_in_bounds(op) {
        return "[UNKNOWN]";
    }

    if let Some((_, description)) = special_opcode_strings(op) {
        return description;
    }

    let idx = op as usize;
    opcode_strings()
        .get(idx)
        .map(|entry| entry.description)
        .unwrap_or("[UNKNOWN]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_table_matches_max() {
        assert_eq!(opcode_strings().len() as i32, OpCode::Max as i32);
    }

    #[test]
    fn opcode_name_description_special_cases() {
        assert_eq!(opcodes_get_name(OpCode::Abort as i32), "OP_ABORT");
        assert_eq!(
            opcodes_get_description(OpCode::Timeout as i32),
            "timeout occurred"
        );
        assert_eq!(opcodes_get_name(OpCode::Repaint as i32), "OP_REPAINT");
        assert_eq!(
            opcodes_get_description(OpCode::Repaint as i32),
            "repaint required"
        );
    }

    #[test]
    fn opcode_unknown_bounds() {
        assert_eq!(opcodes_get_name(-4), "[UNKNOWN]");
        assert_eq!(opcodes_get_description(OpCode::Max as i32), "[UNKNOWN]");
    }

    #[test]
    fn opcode_group_spot_checks() {
        assert_eq!(
            opcodes_get_description(OpCode::AttachmentAttachFile as i32),
            "attach files to this message"
        );
        assert_eq!(
            opcodes_get_description(OpCode::AliasDialog as i32),
            "open the aliases dialog"
        );
        assert_eq!(
            opcodes_get_description(OpCode::DecryptCopy as i32),
            "make decrypted copy"
        );
        assert_eq!(
            opcodes_get_description(OpCode::EnvelopeEditTo as i32),
            "edit the TO list"
        );
        assert_eq!(
            opcodes_get_description(OpCode::ComposePgpMenu as i32),
            "show PGP options"
        );
        assert_eq!(
            opcodes_get_description(OpCode::SidebarToggleVisible as i32),
            "make the sidebar (in)visible"
        );
        assert_eq!(
            opcodes_get_description(OpCode::ComposeSmimeMenu as i32),
            "show S/MIME options"
        );
    }
}
