-- ================================================
-- Media guards: a file guarded from the moment it is uploaded
--
-- `POST /api/media?guard=<plugin>` guards the file at once, so it is not open to every member between the upload and
-- the plugin registering it. Until the plugin calls `media::protect` on the key, only the person who uploaded it may
-- read it (`pending_for`); `media::protect` clears the field.
-- ================================================

DEFINE FIELD IF NOT EXISTS pending_for ON TABLE media_guards TYPE option<string>;
