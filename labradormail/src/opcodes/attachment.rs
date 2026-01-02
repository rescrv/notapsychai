use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_ATTACHMENT_ATTACH_FILE",
        description: "attach files to this message",
    },
    OpString {
        name: "OP_ATTACHMENT_ATTACH_MESSAGE",
        description: "attach messages to this message",
    },
    OpString {
        name: "OP_ATTACHMENT_ATTACH_NEWS_MESSAGE",
        description: "attach news articles to this message",
    },
    OpString {
        name: "OP_ATTACHMENT_COLLAPSE",
        description: "toggle display of subparts",
    },
    OpString {
        name: "OP_ATTACHMENT_DELETE",
        description: "delete the current entry",
    },
    OpString {
        name: "OP_ATTACHMENT_DETACH",
        description: "delete the current entry",
    },
    OpString {
        name: "OP_ATTACHMENT_EDIT_CONTENT_ID",
        description: "edit the 'Content-ID' of the attachment",
    },
    OpString {
        name: "OP_ATTACHMENT_EDIT_DESCRIPTION",
        description: "edit attachment description",
    },
    OpString {
        name: "OP_ATTACHMENT_EDIT_ENCODING",
        description: "edit attachment transfer-encoding",
    },
    OpString {
        name: "OP_ATTACHMENT_EDIT_LANGUAGE",
        description: "edit the 'Content-Language' of the attachment",
    },
    OpString {
        name: "OP_ATTACHMENT_EDIT_MIME",
        description: "edit attachment using mailcap entry",
    },
    OpString {
        name: "OP_ATTACHMENT_EDIT_TYPE",
        description: "edit attachment content type",
    },
    OpString {
        name: "OP_ATTACHMENT_FILTER",
        description: "filter attachment through a shell command",
    },
    OpString {
        name: "OP_ATTACHMENT_GET_ATTACHMENT",
        description: "get a temporary copy of an attachment",
    },
    OpString {
        name: "OP_ATTACHMENT_GROUP_ALTS",
        description: "group tagged attachments as 'multipart/alternative'",
    },
    OpString {
        name: "OP_ATTACHMENT_GROUP_LINGUAL",
        description: "group tagged attachments as 'multipart/multilingual'",
    },
    OpString {
        name: "OP_ATTACHMENT_GROUP_RELATED",
        description: "group tagged attachments as 'multipart/related'",
    },
    OpString {
        name: "OP_ATTACHMENT_MOVE_DOWN",
        description: "move an attachment down in the attachment list",
    },
    OpString {
        name: "OP_ATTACHMENT_MOVE_UP",
        description: "move an attachment up in the attachment list",
    },
    OpString {
        name: "OP_ATTACHMENT_NEW_MIME",
        description: "compose new attachment using mailcap entry",
    },
    OpString {
        name: "OP_ATTACHMENT_PIPE",
        description: "pipe message/attachment to a shell command",
    },
    OpString {
        name: "OP_ATTACHMENT_PRINT",
        description: "print the current entry",
    },
    OpString {
        name: "OP_ATTACHMENT_RENAME_ATTACHMENT",
        description: "send attachment with a different name",
    },
    OpString {
        name: "OP_ATTACHMENT_SAVE",
        description: "save message/attachment to a mailbox/file",
    },
    OpString {
        name: "OP_ATTACHMENT_TOGGLE_DISPOSITION",
        description: "toggle disposition between inline/attachment",
    },
    OpString {
        name: "OP_ATTACHMENT_TOGGLE_RECODE",
        description: "toggle recoding of this attachment",
    },
    OpString {
        name: "OP_ATTACHMENT_TOGGLE_UNLINK",
        description: "toggle whether to delete file after sending it",
    },
    OpString {
        name: "OP_ATTACHMENT_UNDELETE",
        description: "undelete the current entry",
    },
    OpString {
        name: "OP_ATTACHMENT_UNGROUP",
        description: "ungroup 'multipart' attachment",
    },
    OpString {
        name: "OP_ATTACHMENT_UPDATE_ENCODING",
        description: "update an attachment's encoding info",
    },
    OpString {
        name: "OP_ATTACHMENT_VIEW",
        description: "view attachment using mailcap entry if necessary",
    },
    OpString {
        name: "OP_ATTACHMENT_VIEW_MAILCAP",
        description: "force viewing of attachment using mailcap",
    },
    OpString {
        name: "OP_ATTACHMENT_VIEW_PAGER",
        description: "view attachment in pager using copiousoutput mailcap",
    },
    OpString {
        name: "OP_ATTACHMENT_VIEW_TEXT",
        description: "view attachment as text",
    },
    OpString {
        name: "OP_ATTACHMENT_ATTACH_KEY",
        description: "attach a PGP public key",
    },
];
