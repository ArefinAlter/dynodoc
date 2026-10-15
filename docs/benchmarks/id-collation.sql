-- PostgreSQL id and collation benchmark: 26-character ULID-shaped text ids under an
-- ICU locale and under "C" (byte order), against uuid and bigint keys of the same rows.
-- Run it against a disposable database with ICU support (the PostgreSQL 16 image has it):
--   psql "$DISPOSABLE_DATABASE_URL" -f docs/benchmarks/id-collation.sql
-- Every table is temporary. Keys are seeded; run the file again to repeat the timings.
\timing on
select setseed(0.11);
-- Time-ordered prefix (10 characters) and a random tail (16), like ids made by the app.
create temp table raw as
select n,
       md5(n::text)::uuid as g,
       (select string_agg(substr('0123456789ABCDEFGHJKMNPQRSTVWXYZ',
                                 1 + case when i <= 10
                                          then (((1759000000000 + n * 1000::bigint) >> (5 * (10 - i))) & 31)::int
                                          else floor(random() * 32)::int end, 1), '' order by i)
        from generate_series(1, 26) i where n > 0) as u
from generate_series(1, 300000) n;
create temp table probe as select u, g, n from raw order by random() limit 100000;
create temp table blocks as select u, g, n from raw where n <= 8000;
analyze raw; analyze probe; analyze blocks;

\echo === INSERT 300k with primary key (ICU text / C text / uuid / bigint)
create temp table t_icu(id text collate "en-US-x-icu" primary key, v int);
create temp table t_c(id text collate "C" primary key, v int);
create temp table t_uuid(id uuid primary key, v int);
create temp table t_int(id bigint primary key, v int);
insert into t_icu select u, 1 from raw;
insert into t_c select u, 1 from raw;
insert into t_uuid select g, 1 from raw;
insert into t_int select n, 1 from raw;

\echo === EVENT-STYLE index (document, target, seq desc): 300k rows referencing 8k blocks
create temp table e_icu(doc uuid, target text collate "en-US-x-icu", seq int);
create index on e_icu(doc, target, seq desc);
create temp table e_c(doc uuid, target text collate "C", seq int);
create index on e_c(doc, target, seq desc);
create temp table e_uuid(doc uuid, target uuid, seq int);
create index on e_uuid(doc, target, seq desc);
create temp table e_int(doc uuid, target bigint, seq int);
create index on e_int(doc, target, seq desc);
insert into e_icu select '00000000-0000-0000-0000-000000000001', b.u, r.n from raw r join blocks b on b.n = 1 + (r.n % 8000);
insert into e_c select '00000000-0000-0000-0000-000000000001', b.u, r.n from raw r join blocks b on b.n = 1 + (r.n % 8000);
insert into e_uuid select '00000000-0000-0000-0000-000000000001', b.g, r.n from raw r join blocks b on b.n = 1 + (r.n % 8000);
insert into e_int select '00000000-0000-0000-0000-000000000001', 1 + (r.n % 8000), r.n from raw r;
vacuum analyze t_icu; vacuum analyze t_c; vacuum analyze t_uuid; vacuum analyze t_int;

\echo === 100k point lookups through the index (nested loop)
set enable_hashjoin = off; set enable_mergejoin = off;
select count(*) from probe p join t_icu t on t.id = p.u;
select count(*) from probe p join t_c t on t.id = p.u;
select count(*) from probe p join t_uuid t on t.id = p.g;
select count(*) from probe p join t_int t on t.id = p.n;
reset enable_hashjoin; reset enable_mergejoin;

\echo === sort 300k ids without an index
set enable_indexscan = off; set enable_indexonlyscan = off; set enable_bitmapscan = off;
select count(*) from (select id from t_icu order by id offset 0) x;
select count(*) from (select id from t_c order by id offset 0) x;
select count(*) from (select id from t_uuid order by id offset 0) x;
select count(*) from (select id from t_int order by id offset 0) x;
reset enable_indexscan; reset enable_indexonlyscan; reset enable_bitmapscan;

\echo === does ICU order equal byte order for these ids?
select count(*) as positions_that_differ
from (select row_number() over (order by u collate "en-US-x-icu") a,
             row_number() over (order by u collate "C") b from raw) x
where a <> b;
