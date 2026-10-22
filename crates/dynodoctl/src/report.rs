//! Timed, recorded steps: what every long command prints at the end and writes as JSON.

use crate::error::Result;
use crate::util::human_duration;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub name: String,
    pub ok: bool,
    pub millis: u64,
    pub detail: String,
}

#[derive(Default)]
pub struct Recorder {
    pub steps: Vec<Step>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `f`, record how long it took and whether it worked. A failure is recorded and returned.
    pub fn step<T>(&mut self, name: &str, f: impl FnOnce() -> Result<(T, String)>) -> Result<T> {
        let started = Instant::now();
        match f() {
            Ok((value, detail)) => {
                self.steps.push(Step { name: name.into(), ok: true, millis: started.elapsed().as_millis() as u64, detail });
                Ok(value)
            }
            Err(error) => {
                self.steps.push(Step { name: name.into(), ok: false, millis: started.elapsed().as_millis() as u64, detail: error.what.clone() });
                Err(error)
            }
        }
    }

    pub fn note(&mut self, name: &str, ok: bool, detail: &str) {
        self.steps.push(Step { name: name.into(), ok, millis: 0, detail: detail.into() });
    }

    pub fn all_ok(&self) -> bool {
        self.steps.iter().all(|s| s.ok)
    }

    pub fn total_millis(&self) -> u64 {
        self.steps.iter().map(|s| s.millis).sum()
    }
}

pub fn render_steps(steps: &[Step]) -> String {
    let mut out = String::new();
    for s in steps {
        let mark = if s.ok { "ok  " } else { "FAIL" };
        let time = if s.millis > 0 { human_duration(s.millis as u128) } else { String::new() };
        out.push_str(&format!("  {mark} {:<44} {:>9}  {}\n", s.name, time, s.detail));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CliError;

    #[test]
    fn steps_are_recorded_in_order_with_failures() {
        let mut rec = Recorder::new();
        let a = rec.step("first", || Ok((7, "fine".to_string()))).unwrap();
        assert_eq!(a, 7);
        let err = rec.step::<()>("second", || Err(CliError::failed("broke", "fix it"))).unwrap_err();
        assert_eq!(err.what, "broke");
        assert_eq!(rec.steps.len(), 2);
        assert!(rec.steps[0].ok && !rec.steps[1].ok);
        assert!(!rec.all_ok());
        assert!(render_steps(&rec.steps).contains("FAIL second"));
    }
}
