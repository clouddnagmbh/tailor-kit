// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! A server-sent events reader (the streaming format of Anthropic, OpenAI, Azure, Gemini,
//! Mistral and most OpenAI-compatible servers). Chunks arrive at arbitrary byte positions;
//! [`Sse::feed`] returns the events completed so far.

/// One event: its `event:` name (if any) and its `data:` lines joined with `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SseEvent {
    pub name: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub(super) struct Sse {
    /// Bytes of an incomplete line.
    pending: Vec<u8>,
    name: Option<String>,
    data: Vec<String>,
    /// A `\r` ended the last line; a following `\n` belongs to it.
    after_cr: bool,
}

impl Sse {
    pub fn feed(&mut self, chunk: &[u8], out: &mut Vec<SseEvent>) {
        for &b in chunk {
            if self.after_cr {
                self.after_cr = false;
                if b == b'\n' {
                    continue;
                }
            }
            match b {
                b'\r' => {
                    self.after_cr = true;
                    self.line(out);
                }
                b'\n' => self.line(out),
                _ => self.pending.push(b),
            }
        }
    }

    /// The last event of a stream that ended without a blank line.
    pub fn finish(&mut self, out: &mut Vec<SseEvent>) {
        if !self.pending.is_empty() {
            self.line(out);
        }
        self.dispatch(out);
    }

    fn line(&mut self, out: &mut Vec<SseEvent>) {
        let bytes = std::mem::take(&mut self.pending);
        let line = String::from_utf8_lossy(&bytes);
        if line.is_empty() {
            self.dispatch(out);
        } else if line.starts_with(':') {
            // A comment (keep-alive).
        } else {
            let (field, value) = match line.split_once(':') {
                Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
                None => (&*line, ""),
            };
            match field {
                "event" => self.name = Some(value.to_owned()),
                "data" => self.data.push(value.to_owned()),
                _ => {}
            }
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseEvent>) {
        if self.data.is_empty() {
            self.name = None;
            return;
        }
        out.push(SseEvent {
            name: self.name.take(),
            data: std::mem::take(&mut self.data).join("\n"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(chunks: &[&str]) -> Vec<SseEvent> {
        let mut sse = Sse::default();
        let mut out = Vec::new();
        for c in chunks {
            sse.feed(c.as_bytes(), &mut out);
        }
        sse.finish(&mut out);
        out
    }

    #[test]
    fn events_split_anywhere_and_with_any_line_ending() {
        let whole =
            "event: a\ndata: {\"x\":1}\n\n: ping\ndata: l1\ndata: l2\r\n\r\ndata: [DONE]\n\n";
        let expected = events(&[whole]);
        assert_eq!(expected.len(), 3);
        assert_eq!(expected[0].name.as_deref(), Some("a"));
        assert_eq!(expected[1].data, "l1\nl2");
        assert_eq!(expected[2].data, "[DONE]");
        // One byte at a time gives the same events.
        let bytes: Vec<String> = whole.chars().map(String::from).collect();
        let refs: Vec<&str> = bytes.iter().map(String::as_str).collect();
        assert_eq!(events(&refs), expected);
    }

    #[test]
    fn a_last_event_without_a_blank_line_is_kept() {
        assert_eq!(events(&["data: tail"])[0].data, "tail");
    }
}
