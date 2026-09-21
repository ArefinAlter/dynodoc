-- Replies and resolution are canonical, attributed, hash-chained operations.
alter table event drop constraint event_type_check;
alter table event add constraint event_type_check check (type in (
 'NodeCreated','NodeMoved','NodeDeleted','NodeRestored','FieldEdited','RichTextPatched',
 'ChoiceAdded','ChoiceRemoved','CommentAdded','CommentReplied','CommentResolved',
 'SuggestionProposed','SuggestionAccepted','SuggestionRejected','Deployed'
));
create index event_discussion_idx on event(document_id,(payload->>'thread_id'),seq)
    where type in ('CommentReplied','CommentResolved');
