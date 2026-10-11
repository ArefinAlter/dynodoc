-- Named/deployed snapshots may refer to typed shared state. Existing envelopes
-- and their v1 receipts stay immutable and byte-compatible.
alter table snapshot add column state_root text generated always as
    (case when state->>'$dynodoc_state'='2' then state->>'root' end) stored;
alter table snapshot add constraint snapshot_reference_shape check (
    not (state ? '$dynodoc_state') or coalesce(
        state->'$dynodoc_state'='2'::jsonb and jsonb_typeof(state->'root')='string'
        and (state-'$dynodoc_state'-'root')='{}'::jsonb
        and state_root ~ '^[0-9a-f]{64}$', false));
-- Existing audited erasure removes children in a different order. Deferral
-- retains document scoping without changing that transaction's behavior.
alter table snapshot add constraint snapshot_state_reference
    foreign key(document_id,state_root) references checkpoint_object(document_id,address)
    deferrable initially deferred;
alter table snapshot_verification drop constraint snapshot_verification_version_check;
alter table snapshot_verification add constraint snapshot_verification_version_check
    check (version in (1,2));
