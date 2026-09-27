-- Change requests: a personal draft can be submitted for review by the document's
-- editors, like a pull request. Pushing a local file creates such a draft from the
-- revision the file was downloaded at. Draft edits stay append-only; merging still
-- appends ordinary canonical events through the existing write path.
alter table workspace_draft add column submitted_at timestamptz;
alter table workspace_draft add column submission_note text not null default ''
    check (length(submission_note) <= 4000);
-- Where the draft came from, e.g. {"kind":"file","filename":"...","base_reason":"file"}.
alter table workspace_draft add column source jsonb not null default '{}'::jsonb
    check (octet_length(source::text) <= 8192);
alter table workspace_draft add column review_outcome text
    check (review_outcome in ('merged', 'declined', 'withdrawn'));
alter table workspace_draft add column reviewed_by uuid references identity(id);
alter table workspace_draft add column reviewed_at timestamptz;
alter table workspace_draft add column review_note text not null default ''
    check (length(review_note) <= 4000);
create index workspace_draft_submitted_idx on workspace_draft(document_id, submitted_at desc)
    where submitted_at is not null;
