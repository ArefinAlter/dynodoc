-- 0009_views — read-only convenience views.
--
-- PoC scope: only current_documents. The full platform's pending_moderation and
-- verified_citizen_count_per_consultation views belong to the consultation layer
-- (docs 22-31), which is out of scope here.

-- All non-archived documents — the default working set for the researcher UI.
create view current_documents as
    select id, title, languages, status, settings,
           deployed_snapshot_id, created_by, created_at, updated_at
    from document
    where status <> 'archived';
