-- Approvals bound to the revision the reviewer saw (docs/AI-AGENT-CONNECTORS.md 6.2 and 11.3).
--
-- Until now an approval counted when it was written after the contributor's last change
-- (a timestamp comparison). A reviewer who loaded revision 3, while the contributor saved
-- revision 4, still had their click counted for revision 4, content they had not read; and
-- two transactions that start in one order and lock in the other can write a timestamp on
-- the wrong side of the change. After this migration a deciding review (approved or
-- changes_requested) names the exact revision of the request it was given for, the
-- application refuses it when the request has moved, and it counts only while the
-- request's content has not changed since that revision.
--
-- Additive: no existing row changes (amended before it ran in any shared environment).
-- Existing reviews keep a null revision and keep the
-- old rule (written after the last change) for as long as their request lives; they cannot
-- be edited (the table is append-only) and nothing new can be written without a revision.
-- Like 0043 and 0044 this is not rollback-safe by leaving the objects unused: the API
-- refuses a newer schema, so rolling back means restoring the dump.
--
-- 1. The revision of the draft at which its content last changed. `revision` also moves
--    when a manager merges part of the request (the content is unchanged then), so the
--    two are kept apart: an approval counts when its revision is at least this one. A
--    constant default, so no table rewrite.
alter table workspace_draft
    add column content_revision bigint not null default 0 check (content_revision >= 0);

-- 2. The revision a review was given for. Required for a verdict that decides (the
--    constraint is NOT VALID: it checks new rows only and does not scan the table);
--    optional for a comment.
alter table change_request_review
    add column draft_revision bigint check (draft_revision is null or draft_revision >= 0);
alter table change_request_review add constraint change_request_review_decision_revision
    check (verdict = 'commented' or draft_revision is not null) not valid;

-- 2b. Which of one reviewer's decisions is the latest. created_at is the transaction's start
--    time, not its commit order: a reviewer who double-submits from two tabs can have the
--    later commit carry the earlier timestamp and lose. Every writer of a request holds the
--    document lock until commit, so an identity drawn at insert time is in commit order. New
--    rows are ordered by it; rows from before this migration keep their created_at order and
--    count as older than every new row (their draft_revision is null, new decisions have one).
--    The runtime login needs USAGE on the sequence (engine-core database.rs, SEQUENCES).
alter table change_request_review add column seq bigint generated always as identity;

-- 3. The counter is a counter: it never goes back, so (draft, revision) names one state
--    for ever, and the content revision is never ahead of it. Nothing reuses a number.
create function workspace_draft_revision_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if new.revision < old.revision
       or new.content_revision < old.content_revision
       or new.content_revision > new.revision then
        raise exception 'a draft''s revision only moves forward';
    end if;
    return new;
end $$;
create trigger workspace_draft_revision before update on workspace_draft
    for each row execute function workspace_draft_revision_guard();

-- 4. A deciding review can be written only for the revision the draft is at: the draft's
--    row is locked in share mode, so a concurrent change waits for this review to commit
--    and the review then lapses with it. The application checks first and answers 409; this
--    is the second lock on the same door, for any writer that skips the first.
create function change_request_review_revision_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
declare current_revision bigint;
begin
    if new.verdict <> 'commented' then
        select revision into current_revision from public.workspace_draft
            where id = new.draft_id for share;
        if current_revision is distinct from new.draft_revision then
            raise exception 'a review names the revision of the request it was given for (named %, current %)',
                new.draft_revision, current_revision;
        end if;
    end if;
    return new;
end $$;
create trigger change_request_review_revision before insert on change_request_review
    for each row execute function change_request_review_revision_guard();

revoke execute on function workspace_draft_revision_guard(), change_request_review_revision_guard()
    from public;
