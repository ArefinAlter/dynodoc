-- Additive codec capability. Existing immutable manifests keep their v1 roots.
-- Deploy v2-capable readers before opting any publisher into shared-v2. The
-- schema checker rejects older binaries; rollback requires a schema-30 build.
alter table shared_checkpoint drop constraint shared_checkpoint_codec_version_check;
alter table shared_checkpoint add constraint shared_checkpoint_codec_version_check
    check (codec_version in (1,2));
