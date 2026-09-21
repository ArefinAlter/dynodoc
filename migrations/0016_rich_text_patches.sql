-- Additive event vocabulary only. Historic payloads and hashes remain untouched.
alter table event drop constraint event_type_check;
alter table event add constraint event_type_check check (type in (
 'NodeCreated','NodeMoved','NodeDeleted','NodeRestored','FieldEdited','RichTextPatched',
 'ChoiceAdded','ChoiceRemoved','CommentAdded','SuggestionProposed',
 'SuggestionAccepted','SuggestionRejected','Deployed'
));

-- Derived structure/provenance queries use document membership before this index.
create index event_document_target_seq_idx on event (document_id, target_node_id, seq desc);
