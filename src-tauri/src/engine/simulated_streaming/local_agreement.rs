//! LocalAgreement hypothesis buffer for incremental token commitment.
//!
//! Ported from WhisperKit Live's `local_agreement/online_asr.py`.
//!
//! The core idea: run batch transcription repeatedly on a growing audio buffer.
//! Each run produces a hypothesis (list of word tokens). Compare consecutive
//! hypotheses and only emit ("commit") tokens where they agree — the longest
//! common prefix of agreed tokens. This produces stable, non-backtracking
//! incremental output from a batch-only transcription engine.

/// A single word with timing information, produced by batch transcription.
#[derive(Debug, Clone)]
pub struct WordToken {
    pub text: String,
    /// Start time in seconds (absolute, relative to stream start).
    pub start: f64,
    /// End time in seconds (absolute).
    pub end: f64,
    /// Word-level confidence from the ASR engine, if available.
    pub probability: Option<f32>,
}

impl WordToken {
    /// Return a new token with a time offset added to start/end.
    pub fn with_offset(&self, offset: f64) -> Self {
        Self {
            text: self.text.clone(),
            start: self.start + offset,
            end: self.end + offset,
            probability: self.probability,
        }
    }
}

/// Compares consecutive transcription hypotheses and commits only tokens
/// where two successive runs agree (longest common prefix match).
pub struct HypothesisBuffer {
    /// All committed tokens for the current session (used for prompt context).
    committed: Vec<WordToken>,
    /// Previous hypothesis tail — the uncommitted tokens from last round,
    /// used as the baseline for comparison with the next hypothesis.
    buffer: Vec<WordToken>,
    /// End time of the most recently committed token.
    last_committed_time: f64,
}

impl HypothesisBuffer {
    pub fn new() -> Self {
        Self {
            committed: Vec::new(),
            buffer: Vec::new(),
            last_committed_time: 0.0,
        }
    }

    /// Insert a new hypothesis from the latest transcription run.
    ///
    /// `new_tokens` — word tokens from the current transcription (times relative
    ///   to the start of the audio buffer that was transcribed).
    /// `offset` — the absolute time offset of the audio buffer's start, so we can
    ///   convert token times to absolute stream times.
    ///
    /// This filters out tokens that overlap with already-committed content and
    /// removes n-gram matches (up to 5 tokens) between the committed tail and
    /// the new hypothesis head.
    pub fn insert(&mut self, new_tokens: Vec<WordToken>, offset: f64) {
        // Apply time offset to convert to absolute times.
        let mut new: Vec<WordToken> = new_tokens
            .into_iter()
            .map(|t| t.with_offset(offset))
            .collect();

        // Filter out tokens that overlap with already-committed content.
        // Keep tokens starting after (last_committed_time - 0.1s) tolerance.
        let cutoff = self.last_committed_time - 0.1;
        new.retain(|t| t.start > cutoff);

        // N-gram deduplication: if the last N committed tokens match the first N
        // new tokens (by text), remove those from new to avoid duplication.
        if !new.is_empty() && !self.committed.is_empty() {
            let first_new_start = new[0].start;
            // Only do n-gram matching if the new tokens are temporally close
            // to the last committed token (within 1 second).
            if first_new_start <= self.last_committed_time + 1.0 {
                let max_ngram = 5.min(self.committed.len()).min(new.len());
                let mut best_match = 0;
                for n in 1..=max_ngram {
                    let committed_ngram: String = self.committed
                        [self.committed.len() - n..]
                        .iter()
                        .map(|t| t.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let new_ngram: String = new[..n]
                        .iter()
                        .map(|t| t.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    if committed_ngram == new_ngram {
                        best_match = n;
                    }
                }
                if best_match > 0 {
                    new.drain(..best_match);
                }
            }
        }

        // Store for the flush phase. The current `self.buffer` (previous hypothesis
        // tail) is kept for comparison; `new` becomes the candidate.
        // We swap: buffer holds old tail, new_tokens field holds current candidate.
        // The flush() method will compare them.
        //
        // Note: we temporarily stash `new` in a way flush() can access it.
        // We do this by extending the struct with a `pending` field.
        self.flush_inner(new);
    }

    /// Compare the new hypothesis against the previous buffer (longest common
    /// prefix), commit agreed tokens, and rotate state for the next round.
    ///
    /// Returns the newly committed tokens.
    fn flush_inner(&mut self, mut new: Vec<WordToken>) -> Vec<WordToken> {
        let mut committed = Vec::new();
        let mut buf_idx = 0;

        while !new.is_empty() {
            if buf_idx >= self.buffer.len() {
                // No more previous hypothesis to compare against — stop.
                break;
            }
            if new[0].text == self.buffer[buf_idx].text {
                // Agreement! Commit this token.
                let token = new.remove(0);
                self.last_committed_time = token.end;
                committed.push(token);
                buf_idx += 1;
            } else {
                // First disagreement — stop committing.
                break;
            }
        }

        // Rotate: current `new` (remaining uncommitted) becomes the baseline
        // for the next comparison round.
        self.buffer = new;
        self.committed.extend(committed.clone());
        committed
    }

    /// Insert a new hypothesis and flush in one step. Returns newly committed tokens.
    pub fn insert_and_flush(
        &mut self,
        new_tokens: Vec<WordToken>,
        offset: f64,
    ) -> Vec<WordToken> {
        // Apply time offset.
        let mut new: Vec<WordToken> = new_tokens
            .into_iter()
            .map(|t| t.with_offset(offset))
            .collect();

        // Filter overlapping tokens.
        let cutoff = self.last_committed_time - 0.1;
        new.retain(|t| t.start > cutoff);

        // N-gram dedup.
        if !new.is_empty() && !self.committed.is_empty() {
            let first_new_start = new[0].start;
            if first_new_start <= self.last_committed_time + 1.0 {
                let max_ngram = 5.min(self.committed.len()).min(new.len());
                let mut best_match = 0;
                for n in 1..=max_ngram {
                    let committed_ngram: String = self.committed
                        [self.committed.len() - n..]
                        .iter()
                        .map(|t| t.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let new_ngram: String = new[..n]
                        .iter()
                        .map(|t| t.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    if committed_ngram == new_ngram {
                        best_match = n;
                    }
                }
                if best_match > 0 {
                    new.drain(..best_match);
                }
            }
        }

        self.flush_inner(new)
    }

    /// Remove committed tokens older than `time`. Called during audio buffer
    /// trimming to prevent unbounded growth of the committed list.
    pub fn pop_committed(&mut self, time: f64) {
        self.committed.retain(|t| t.end > time);
    }

    /// Access all committed tokens (for prompt generation).
    pub fn committed(&self) -> &[WordToken] {
        &self.committed
    }

    /// The end time of the last committed token.
    pub fn last_committed_time(&self) -> f64 {
        self.last_committed_time
    }

    /// Force-flush: commit everything remaining in the buffer.
    /// Used at end of stream when there's no next hypothesis to compare against.
    pub fn force_flush(&mut self) -> Vec<WordToken> {
        let tokens: Vec<WordToken> = self.buffer.drain(..).collect();
        if let Some(last) = tokens.last() {
            self.last_committed_time = last.end;
        }
        self.committed.extend(tokens.clone());
        tokens
    }

    /// Build a prompt string from committed tokens (last ~200 chars).
    /// This is passed as the `initial_prompt` to the next transcription call
    /// for context continuity.
    pub fn build_prompt(&self) -> Option<String> {
        if self.committed.is_empty() {
            return None;
        }

        let mut words: Vec<&str> = Vec::new();
        let mut char_count = 0;

        // Walk backwards through committed tokens, collecting up to 200 chars.
        for token in self.committed.iter().rev() {
            let len = token.text.len() + 1; // +1 for space
            if char_count + len > 200 {
                break;
            }
            char_count += len;
            words.push(&token.text);
        }

        words.reverse();
        if words.is_empty() {
            None
        } else {
            Some(words.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str, start: f64, end: f64) -> WordToken {
        WordToken {
            text: text.to_string(),
            start,
            end,
            probability: None,
        }
    }

    #[test]
    fn first_hypothesis_commits_nothing() {
        let mut buf = HypothesisBuffer::new();
        let tokens = vec![token("hello", 0.0, 0.5), token("world", 0.5, 1.0)];
        let committed = buf.insert_and_flush(tokens, 0.0);
        // First hypothesis has no previous buffer to compare against.
        assert!(committed.is_empty());
    }

    #[test]
    fn agreeing_hypotheses_commit_common_prefix() {
        let mut buf = HypothesisBuffer::new();

        // First hypothesis: "hello world"
        let h1 = vec![token("hello", 0.0, 0.5), token("world", 0.5, 1.0)];
        buf.insert_and_flush(h1, 0.0);

        // Second hypothesis: "hello world foo"
        let h2 = vec![
            token("hello", 0.0, 0.5),
            token("world", 0.5, 1.0),
            token("foo", 1.0, 1.5),
        ];
        let committed = buf.insert_and_flush(h2, 0.0);

        assert_eq!(committed.len(), 2);
        assert_eq!(committed[0].text, "hello");
        assert_eq!(committed[1].text, "world");
    }

    #[test]
    fn disagreeing_hypotheses_stop_at_first_diff() {
        let mut buf = HypothesisBuffer::new();

        // First: "the cat sat"
        let h1 = vec![
            token("the", 0.0, 0.3),
            token("cat", 0.3, 0.6),
            token("sat", 0.6, 1.0),
        ];
        buf.insert_and_flush(h1, 0.0);

        // Second: "the dog ran" (disagrees at position 1)
        let h2 = vec![
            token("the", 0.0, 0.3),
            token("dog", 0.3, 0.6),
            token("ran", 0.6, 1.0),
        ];
        let committed = buf.insert_and_flush(h2, 0.0);

        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].text, "the");
    }

    #[test]
    fn completely_different_hypotheses_commit_nothing() {
        let mut buf = HypothesisBuffer::new();

        let h1 = vec![token("hello", 0.0, 0.5)];
        buf.insert_and_flush(h1, 0.0);

        let h2 = vec![token("goodbye", 0.0, 0.5)];
        let committed = buf.insert_and_flush(h2, 0.0);

        assert!(committed.is_empty());
    }

    #[test]
    fn offset_is_applied_to_token_times() {
        let mut buf = HypothesisBuffer::new();

        let h1 = vec![token("hello", 0.0, 0.5)];
        buf.insert_and_flush(h1, 5.0); // offset = 5s

        let h2 = vec![token("hello", 0.0, 0.5), token("world", 0.5, 1.0)];
        let committed = buf.insert_and_flush(h2, 5.0);

        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].start, 5.0); // 0.0 + 5.0 offset
        assert_eq!(committed[0].end, 5.5); // 0.5 + 5.0 offset
    }

    #[test]
    fn ngram_dedup_removes_overlap() {
        let mut buf = HypothesisBuffer::new();

        // First hypothesis commits "hello world" via two rounds.
        let h1 = vec![token("hello", 0.0, 0.5), token("world", 0.5, 1.0)];
        buf.insert_and_flush(h1, 0.0);

        let h2 = vec![
            token("hello", 0.0, 0.5),
            token("world", 0.5, 1.0),
            token("foo", 1.0, 1.5),
        ];
        let committed = buf.insert_and_flush(h2, 0.0);
        // "hello" and "world" committed here.
        assert_eq!(committed.len(), 2);

        // Third hypothesis repeats "world foo bar" — "world" should be deduped.
        let h3 = vec![
            token("world", 0.5, 1.0),
            token("foo", 1.0, 1.5),
            token("bar", 1.5, 2.0),
        ];
        let committed = buf.insert_and_flush(h3, 0.0);

        // "world" was already committed and should be deduped by n-gram matching.
        // "foo" agrees with buffer, so it gets committed.
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].text, "foo");
    }

    #[test]
    fn pop_committed_removes_old_tokens() {
        let mut buf = HypothesisBuffer::new();

        let h1 = vec![token("a", 0.0, 1.0), token("b", 1.0, 2.0)];
        buf.insert_and_flush(h1, 0.0);
        let h2 = vec![
            token("a", 0.0, 1.0),
            token("b", 1.0, 2.0),
            token("c", 2.0, 3.0),
        ];
        buf.insert_and_flush(h2, 0.0);

        assert_eq!(buf.committed().len(), 2); // "a" and "b"
        buf.pop_committed(1.5); // Remove tokens ending <= 1.5
        assert_eq!(buf.committed().len(), 1); // Only "b" (end=2.0) remains
        assert_eq!(buf.committed()[0].text, "b");
    }

    #[test]
    fn force_flush_commits_remaining_buffer() {
        let mut buf = HypothesisBuffer::new();

        let h1 = vec![token("hello", 0.0, 0.5), token("world", 0.5, 1.0)];
        buf.insert_and_flush(h1, 0.0);

        // Buffer now contains ["hello", "world"] as uncommitted baseline.
        let remaining = buf.force_flush();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].text, "hello");
        assert_eq!(remaining[1].text, "world");
    }

    #[test]
    fn build_prompt_returns_last_200_chars() {
        let mut buf = HypothesisBuffer::new();

        // Create enough tokens to exceed 200 chars.
        let h1: Vec<WordToken> = (0..50)
            .map(|i| token(&format!("word{i}"), i as f64 * 0.1, (i + 1) as f64 * 0.1))
            .collect();
        buf.insert_and_flush(h1, 0.0);

        let h2: Vec<WordToken> = (0..50)
            .map(|i| token(&format!("word{i}"), i as f64 * 0.1, (i + 1) as f64 * 0.1))
            .collect();
        buf.insert_and_flush(h2, 0.0);

        let prompt = buf.build_prompt();
        assert!(prompt.is_some());
        assert!(prompt.unwrap().len() <= 210); // Approximate, allows for word boundary
    }

    #[test]
    fn empty_buffer_prompt_returns_none() {
        let buf = HypothesisBuffer::new();
        assert!(buf.build_prompt().is_none());
    }

    #[test]
    fn last_committed_time_tracks_correctly() {
        let mut buf = HypothesisBuffer::new();

        let h1 = vec![token("a", 0.0, 0.5)];
        buf.insert_and_flush(h1, 0.0);
        assert_eq!(buf.last_committed_time(), 0.0); // Nothing committed yet

        let h2 = vec![token("a", 0.0, 0.5), token("b", 0.5, 1.0)];
        buf.insert_and_flush(h2, 0.0);
        assert_eq!(buf.last_committed_time(), 0.5); // "a" committed (end=0.5)
    }
}
