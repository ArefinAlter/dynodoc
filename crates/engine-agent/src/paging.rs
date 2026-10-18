//! One place that turns rendered units into a bounded, paged response (spec 6.10, 11):
//! header assembly, the byte budget, the continuation cursor, `warn=` and `_meta`.

use std::collections::BTreeMap;

use crate::budget::{fit, Cursor, Unit};
use crate::dct::{HeaderBuilder, Tool};
use crate::error::AgentError;
use crate::escape::Dropped;
use crate::model::FileMeta;
use crate::response::{Meta, Response, Warnings};

/// What differs between the list-like tools.
pub struct PagedSpec<'a> {
    pub tool: Tool,
    /// `None` for `files`, which spans files.
    pub file: Option<&'a FileMeta>,
    /// Pairs after `file rev chain`.
    pub lead: Vec<(&'static str, String)>,
    /// Add `n=<units in this page>` after `lead`.
    pub show_n: bool,
    /// Pairs after `n`, before `more`.
    pub tail: Vec<(&'static str, String)>,
    /// Warnings known before paging (`redacted`, `stale`).
    pub base_warn: Warnings,
    /// Body lines present in every page (column header, sheet name).
    pub fixed: String,
    pub fixed_dropped: Dropped,
    /// Number carried in the cursor next to `(rev, position)`.
    pub cursor_aux: Option<u64>,
}

fn cumulative(units: &[Unit]) -> Vec<Dropped> {
    let mut out = Vec::with_capacity(units.len() + 1);
    let mut acc = Dropped::default();
    out.push(acc);
    for u in units {
        acc.add(u.dropped);
        out.push(acc);
    }
    out
}

/// Fit `units[start..]` into the budget and build the response. `data` receives the
/// number of units taken and the continuation cursor.
pub fn paged<T>(
    spec: PagedSpec<'_>,
    units: &[Unit],
    start: usize,
    limit: usize,
    max_bytes: usize,
    data: impl FnOnce(usize, Option<Cursor>) -> T,
) -> Result<Response<T>, AgentError> {
    let rev = spec.file.map_or(0, |f| f.rev);
    let cum = cumulative(units);
    let start = start.min(units.len());
    let header = |taken: usize, more: bool| -> HeaderBuilder {
        let mut h = HeaderBuilder::new(spec.tool);
        if let Some(f) = spec.file {
            h = h.kv("file", f.short_id()).kv("rev", f.rev).kv("chain", f.chain8());
        }
        for (k, v) in &spec.lead {
            h = h.kv(k, v);
        }
        if spec.show_n {
            h = h.kv("n", taken);
        }
        for (k, v) in &spec.tail {
            h = h.kv(k, v);
        }
        if more {
            h = h.kv(
                "more",
                Cursor {
                    rev,
                    pos: (start + taken) as u64,
                    aux: spec.cursor_aux,
                },
            );
        }
        let mut w = spec.base_warn;
        w.truncated = more;
        w.dropped = spec.fixed_dropped;
        let (a, b) = (cum[start], cum[start + taken]);
        w.dropped.ctrl += b.ctrl - a.ctrl;
        w.dropped.hidden += b.hidden - a.hidden;
        w.apply(h).kv("trust", "data")
    };
    let taken = fit(units, start, limit, max_bytes, spec.fixed.len(), header)?;
    let more = start + taken < units.len();
    let page = &units[start..start + taken];
    let mut body = spec.fixed.clone();
    for u in page {
        body.push_str(&u.text);
    }
    let h = header(taken, more);
    let text = h.assemble(&body);
    let cursor = more.then_some(Cursor {
        rev,
        pos: (start + taken) as u64,
        aux: spec.cursor_aux,
    });
    let mut ids = BTreeMap::new();
    let mut warn = spec.base_warn;
    warn.truncated = more;
    warn.dropped = spec.fixed_dropped;
    for u in page {
        warn.dropped.add(u.dropped);
        for (r, id) in &u.ids {
            ids.insert(r.as_str().to_string(), id.0.clone());
        }
    }
    let meta_cursor = cursor.map(|c| c.to_string());
    let meta = match spec.file {
        Some(f) => Meta::new(f, text.len(), max_bytes, meta_cursor, ids, &warn),
        None => Meta::list(text.len(), max_bytes, meta_cursor, ids, &warn),
    };
    Ok(Response {
        text,
        meta,
        data: data(taken, cursor),
    })
}
