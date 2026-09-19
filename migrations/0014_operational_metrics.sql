-- Anonymous per-page samples; no IPs, identity IDs, URLs, queries or document content.
-- Independent of the immutable research event log. Retained for 30 days by ops cron.
create table web_metric (
    page_id uuid not null,
    metric text not null check(metric in ('visit','LCP','INP','CLS','FCP','TTFB')),
    host text not null check(host in ('homepage','workspace','admin')),
    page text not null check(page in ('home','privacy','terms','login','workspace','editor','admin','other')),
    value double precision not null check(value >= 0 and value <= 3600000 and value <> 'NaN'::float8),
    recorded_at timestamptz not null default now(),
    primary key(page_id, metric)
);
create index web_metric_recorded_idx on web_metric(recorded_at);
