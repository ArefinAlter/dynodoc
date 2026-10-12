-- Local SQLite schema 1 -> 2, run once inside the initialization transaction.
-- Retained objects include staged/unreachable content. No history is deleted.
create table local_storage (
    singleton integer primary key check(singleton=1),
    bytes_used integer not null check(bytes_used>=0),
    objects_used integer not null check(objects_used>=0),
    commits_used integer not null check(commits_used>=0),
    branches_used integer not null check(branches_used>=0),
    byte_limit integer not null check(byte_limit>=bytes_used),
    object_limit integer not null check(object_limit>=objects_used),
    commit_limit integer not null check(commit_limit>=commits_used),
    branch_limit integer not null check(branch_limit>=branches_used)
);

-- Preserve stores already above a default: freeze that dimension at existing
-- usage until the owner explicitly increases the limit. This scan runs once.
insert into local_storage
select 1,bytes,objects,commits,branches,
       max(4294967296,bytes),max(1000000,objects),max(100000,commits),max(4096,branches)
from (
    select coalesce((select sum(length(bytes)) from objects),0)
             + coalesce((select sum(length(body)) from commits),0) as bytes,
           (select count(*) from objects) as objects,
           (select count(*) from commits) as commits,
           (select count(*) from refs) as branches
);

-- AFTER INSERT charges actual rows only. Reused content/commit retries remain
-- free at capacity. RAISE(ABORT) and the caller's rollback make a batch atomic.
create trigger objects_storage after insert on objects begin
    update local_storage set bytes_used=bytes_used+length(new.bytes),objects_used=objects_used+1
      where singleton=1 and length(new.bytes)<=byte_limit-bytes_used and objects_used<object_limit;
    select case when changes()=0 then raise(abort,'local repository capacity exceeded') end;
end;
create trigger commits_storage after insert on commits begin
    update local_storage set bytes_used=bytes_used+length(new.body),commits_used=commits_used+1
      where singleton=1 and length(new.body)<=byte_limit-bytes_used and commits_used<commit_limit;
    select case when changes()=0 then raise(abort,'local repository capacity exceeded') end;
end;
create trigger refs_storage after insert on refs begin
    update local_storage set branches_used=branches_used+1
      where singleton=1 and branches_used<branch_limit;
    select case when changes()=0 then raise(abort,'local repository capacity exceeded') end;
end;
-- Branch deletion is not exposed yet. Keep accounting correct if added later.
create trigger refs_storage_delete after delete on refs begin
    update local_storage set branches_used=branches_used-1 where singleton=1;
end;
update local_meta set version=2 where singleton=1;
