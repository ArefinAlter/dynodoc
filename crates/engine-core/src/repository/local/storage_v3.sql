-- Local SQLite schema 2 -> 3, run once inside the initialization transaction.
-- Per-object compression at rest. No object, address, commit, signature or ref is
-- rewritten: existing rows become identity rows (codec 0, size NULL) in place.
--
-- `size` is the logical (canonical encoded) length that quotas, accounting and
-- addresses use; `bytes` is the stored form. NULL size means a row written before
-- this schema, whose logical length is length(bytes). Compressed rows must declare
-- a size within the 1 MiB object limit and be strictly smaller than it. Codec 1 is
-- raw DEFLATE; any other positive number is reserved, refused on read by software
-- that does not know it, and needs no table rebuild to introduce.
alter table objects add column codec integer not null default 0 check(codec>=0);
alter table objects add column size integer check(
    (codec=0 and (size is null or size=length(bytes)))
    or (codec>0 and size between 2 and 1048576 and length(bytes)<size)
);

-- Physical payload bytes (object rows as stored, plus commit bodies). The existing
-- bytes_used keeps counting logical bytes, so limits and plans mean what they did.
alter table local_storage add column stored_bytes integer not null default 0 check(stored_bytes>=0);
update local_storage set stored_bytes=bytes_used where singleton=1;

-- Objects stay immutable. The only permitted update is `repository compact`, which
-- inserts this row inside its own transaction, changes only how a row is stored
-- (never its document, address or logical length), and removes the row before it
-- commits, so no other connection ever sees it.
create table local_compaction (singleton integer primary key check(singleton=1));
drop trigger objects_no_update;
create trigger objects_no_update before update on objects
when not exists(select 1 from local_compaction)
  or old.document is not new.document
  or old.address is not new.address
  or coalesce(old.size,length(old.bytes)) is not coalesce(new.size,length(new.bytes))
begin select raise(abort,'immutable object'); end;

drop trigger objects_storage;
create trigger objects_storage after insert on objects begin
    update local_storage
      set bytes_used=bytes_used+coalesce(new.size,length(new.bytes)),
          stored_bytes=stored_bytes+length(new.bytes),
          objects_used=objects_used+1
      where singleton=1
        and coalesce(new.size,length(new.bytes))<=byte_limit-bytes_used
        and objects_used<object_limit;
    select case when changes()=0 then raise(abort,'local repository capacity exceeded') end;
end;
create trigger objects_storage_repack after update on objects begin
    update local_storage
      set stored_bytes=stored_bytes-length(old.bytes)+length(new.bytes)
      where singleton=1;
end;

drop trigger commits_storage;
create trigger commits_storage after insert on commits begin
    update local_storage
      set bytes_used=bytes_used+length(new.body),
          stored_bytes=stored_bytes+length(new.body),
          commits_used=commits_used+1
      where singleton=1 and length(new.body)<=byte_limit-bytes_used and commits_used<commit_limit;
    select case when changes()=0 then raise(abort,'local repository capacity exceeded') end;
end;

update local_meta set version=3 where singleton=1;
