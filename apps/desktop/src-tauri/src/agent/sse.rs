//! A server-sent events decoder: bytes in, events out. Bounded (one line is at most 1 MiB), tolerant
//! of `\r\n`, comments and fields it does not use. No network in here.

const MAX_LINE: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SseError {
    LineTooLong,
}

#[derive(Default)]
pub struct SseDecoder {
    line: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == b'\n' {
                self.end_line(&mut out);
            } else {
                if self.line.len() >= MAX_LINE {
                    return Err(SseError::LineTooLong);
                }
                self.line.push(b);
            }
        }
        Ok(out)
    }

    /// The stream ended: an event the server did not finish with a blank line still counts.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if !self.line.is_empty() {
            self.end_line(&mut out);
        }
        self.dispatch(&mut out);
        out
    }

    fn end_line(&mut self, out: &mut Vec<SseEvent>) {
        let raw = std::mem::take(&mut self.line);
        let text = String::from_utf8_lossy(&raw);
        let line = text.strip_suffix('\r').unwrap_or(&text);
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseEvent>) {
        if self.data.is_empty() {
            self.event = None;
            return;
        }
        out.push(SseEvent {
            event: self.event.take(),
            data: self.data.join("\n"),
        });
        self.data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_split_on_blank_lines_across_chunk_boundaries() {
        let mut d = SseDecoder::default();
        let mut all = Vec::new();
        for chunk in [&b"event: message_start\nda"[..], b"ta: {\"a\":1}\n\n: ping\n\nda", b"ta: one\r\ndata: two\r\n\r\n"] {
            all.extend(d.push(chunk).unwrap());
        }
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].event.as_deref(), Some("message_start"));
        assert_eq!(all[0].data, "{\"a\":1}");
        assert_eq!(all[1].event, None);
        assert_eq!(all[1].data, "one\ntwo");
    }

    #[test]
    fn an_unfinished_event_is_flushed_at_the_end_and_a_huge_line_is_refused() {
        let mut d = SseDecoder::default();
        assert!(d.push(b"data: tail").unwrap().is_empty());
        let end = d.finish();
        assert_eq!(end.len(), 1);
        assert_eq!(end[0].data, "tail");
        let mut d = SseDecoder::default();
        assert_eq!(d.push(&vec![b'x'; MAX_LINE + 5]), Err(SseError::LineTooLong));
    }

    #[test]
    fn comments_and_unknown_fields_are_ignored() {
        let mut d = SseDecoder::default();
        let ev = d.push(b": hello\nid: 7\nretry: 100\ndata:[DONE]\n\n").unwrap();
        assert_eq!(ev, vec![SseEvent { event: None, data: "[DONE]".into() }]);
    }
}
