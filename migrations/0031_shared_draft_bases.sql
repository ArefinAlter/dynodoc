-- New drafts may store a typed state reference instead of a full JSON state.
-- Existing drafts remain readable and are never rewritten by this migration.
alter table workspace_draft add column base_state_root text generated always as
    (case when base_state->>'$dynodoc_state'='2' then base_state->>'root' end) stored;
alter table workspace_draft add column merge_base_root text generated always as
    (case when merge_base_state->>'$dynodoc_state'='2' then merge_base_state->>'root' end) stored;
alter table workspace_draft add constraint draft_base_reference_shape check (
    not (base_state ? '$dynodoc_state') or coalesce(
        base_state->'$dynodoc_state'='2'::jsonb and jsonb_typeof(base_state->'root')='string'
        and (base_state-'$dynodoc_state'-'root')='{}'::jsonb
        and base_state_root ~ '^[0-9a-f]{64}$', false));
alter table workspace_draft add constraint draft_merge_reference_shape check (
    not (merge_base_state ? '$dynodoc_state') or coalesce(
        merge_base_state->'$dynodoc_state'='2'::jsonb and jsonb_typeof(merge_base_state->'root')='string'
        and (merge_base_state-'$dynodoc_state'-'root')='{}'::jsonb
        and merge_base_root ~ '^[0-9a-f]{64}$', false));
-- Deferral permits the established audited erasure triggers to remove all
-- children in their existing order. Scope remains enforced at transaction commit.
alter table workspace_draft add constraint draft_base_reference foreign key(document_id,base_state_root)
    references checkpoint_object(document_id,address) deferrable initially deferred;
alter table workspace_draft add constraint draft_merge_reference foreign key(document_id,merge_base_root)
    references checkpoint_object(document_id,address) deferrable initially deferred;
create trigger workspace_draft_base_immutable before update of base_state on workspace_draft
    for each row when (old.base_state is distinct from new.base_state) execute function event_immutable();
