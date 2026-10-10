-- Minimal authenticated profiles. No public directory or email projection.
alter table identity add column bio text not null default '' check (length(bio) <= 280);
alter table identity add column profile_revision bigint not null default 0 check (profile_revision >= 0);
