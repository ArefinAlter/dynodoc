-- Read-only activity APIs: project activity feed, contributor totals, profile
-- contribution calendars and administrator audit review. Indexes only. No row,
-- grant, function, trigger, hash-chain or immutable-history change; runtime
-- privileges are unchanged and old API builds ignore these indexes.
--
-- Project feed keyset pages walk each document newest-first by (created_at, id);
-- contributor totals read the author from the same entries without the table.
create index event_document_created_idx
    on event(document_id, created_at desc, (id collate "C") desc) include (actor_id);
-- Contribution calendars count one account's events, optionally restricted to
-- documents, without scanning every event.
create index event_actor_created_idx
    on event(actor_id, created_at) include (document_id);
-- Merge, access-change and last-change lookups address audit rows by resource.
create index operation_audit_resource_idx
    on operation_audit(resource_id, id desc);
