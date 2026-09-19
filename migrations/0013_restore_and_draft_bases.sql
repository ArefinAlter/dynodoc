-- Restoration is an explicit audited operation; stable IDs and old events survive.
alter table event drop constraint event_type_check;
alter table event add constraint event_type_check check (type in (
 'NodeCreated','NodeMoved','NodeDeleted','NodeRestored','FieldEdited',
 'ChoiceAdded','ChoiceRemoved','CommentAdded','SuggestionProposed',
 'SuggestionAccepted','SuggestionRejected','Deployed'
));
alter table workspace_draft add column merge_base_state jsonb;
alter table workspace_draft add column revision bigint not null default 0;
