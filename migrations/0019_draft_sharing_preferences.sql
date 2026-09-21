-- Private sharing preferences are draft metadata, never modifications to events.
alter table workspace_draft add column excluded_nodes text[] not null default '{}';
alter table workspace_draft add constraint draft_exclusions_bounded check(cardinality(excluded_nodes)<=20000);
