use natural::phonetics::soundex;
use strsim::levenshtein;
use tracing::info;

/// Default fuzzy-matching threshold.
/// Lower = stricter. 0.18 works well for speech recognition corrections.
pub const DEFAULT_THRESHOLD: f64 = 0.18;

/// Apply custom word corrections to transcribed text using fuzzy matching.
///
/// Uses Levenshtein distance + Soundex phonetic matching + n-gram matching
/// to automatically find and correct misheard words — no manual aliases needed.
pub fn apply_custom_words(text: &str, custom_words: &[String], threshold: f64) -> String {
    if custom_words.is_empty() || text.is_empty() {
        return text.to_string();
    }

    // Pre-compute lowercase and no-space versions for comparison
    let custom_words_lower: Vec<String> = custom_words.iter().map(|w| w.to_lowercase()).collect();
    let custom_words_nospace: Vec<String> = custom_words_lower
        .iter()
        .map(|w| w.replace(' ', ""))
        .collect();

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result = Vec::new();
    let mut i = 0;
    let mut replacements = 0u32;

    while i < words.len() {
        let mut matched = false;

        // Try n-grams from longest (3) to shortest (1) — greedy longest match
        for n in (1..=3).rev() {
            if i + n > words.len() {
                continue;
            }

            let ngram_words = &words[i..i + n];
            let ngram = build_ngram(ngram_words);

            if let Some((replacement, _score)) =
                find_best_match(&ngram, custom_words, &custom_words_nospace, threshold)
            {
                let (prefix, _) = extract_punctuation(ngram_words[0]);
                let (_, suffix) = extract_punctuation(ngram_words[n - 1]);

                let corrected = preserve_case_pattern(ngram_words[0], replacement);
                result.push(format!("{prefix}{corrected}{suffix}"));
                i += n;
                matched = true;
                replacements += 1;
                break;
            }
        }

        if !matched {
            result.push(words[i].to_string());
            i += 1;
        }
    }

    if replacements > 0 {
        info!(replacements, "Applied custom word corrections");
    }

    result.join(" ")
}

/// Build an n-gram string by stripping punctuation, lowercasing, and concatenating.
/// This lets "Charge B" match against "ChargeBee".
fn build_ngram(words: &[&str]) -> String {
    words
        .iter()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .collect::<Vec<_>>()
        .concat()
}

/// Find the best matching custom word for a candidate string.
///
/// Uses Levenshtein distance and Soundex phonetic matching.
/// Returns the best match and its score, if any match is within threshold.
fn find_best_match<'a>(
    candidate: &str,
    custom_words: &'a [String],
    custom_words_nospace: &[String],
    threshold: f64,
) -> Option<(&'a String, f64)> {
    if candidate.is_empty() || candidate.len() > 50 {
        return None;
    }

    let mut best_match: Option<&String> = None;
    let mut best_score = f64::MAX;

    for (i, custom_word_nospace) in custom_words_nospace.iter().enumerate() {
        // Skip if lengths are too different (max 15% difference, minimum 1 char)
        // Tight constraint prevents n-grams from absorbing unrelated short words
        let len_diff = (candidate.len() as i32 - custom_word_nospace.len() as i32).abs() as f64;
        let max_len = candidate.len().max(custom_word_nospace.len()) as f64;
        let max_allowed_diff = (max_len * 0.15).max(1.0);
        if len_diff > max_allowed_diff {
            continue;
        }

        // Normalized Levenshtein distance
        let lev_dist = levenshtein(candidate, custom_word_nospace);
        let lev_score = if max_len > 0.0 {
            lev_dist as f64 / max_len
        } else {
            1.0
        };

        // Phonetic similarity via Soundex
        let phonetic_match = soundex(candidate, custom_word_nospace);

        // Combined score: significant boost for phonetic matches
        let combined_score = if phonetic_match {
            lev_score * 0.3
        } else {
            lev_score
        };

        if combined_score < threshold && combined_score < best_score {
            best_match = Some(&custom_words[i]);
            best_score = combined_score;
        }
    }

    best_match.map(|m| (m, best_score))
}

/// Preserve the case pattern of the original word when applying a replacement.
fn preserve_case_pattern(original: &str, replacement: &str) -> String {
    let clean = original.trim_matches(|c: char| !c.is_alphanumeric());
    if clean.chars().all(|c| c.is_uppercase()) {
        replacement.to_uppercase()
    } else if clean.chars().next().map_or(false, |c| c.is_uppercase()) {
        let mut chars: Vec<char> = replacement.chars().collect();
        if let Some(first) = chars.get_mut(0) {
            *first = first.to_uppercase().next().unwrap_or(*first);
        }
        chars.into_iter().collect()
    } else {
        replacement.to_string()
    }
}

/// Extract punctuation prefix and suffix from a word.
fn extract_punctuation(word: &str) -> (&str, &str) {
    let prefix_end = word
        .chars()
        .take_while(|c| !c.is_alphanumeric())
        .count();
    let suffix_start = word
        .char_indices()
        .rev()
        .take_while(|(_, c)| !c.is_alphanumeric())
        .count();

    let prefix = if prefix_end > 0 {
        &word[..prefix_end]
    } else {
        ""
    };

    let suffix = if suffix_start > 0 {
        &word[word.len() - suffix_start..]
    } else {
        ""
    };

    (prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_corrects() {
        let words = vec!["EchoType".to_string()];
        let result = apply_custom_words("I use echotype daily", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "I use EchoType daily");
    }

    #[test]
    fn fuzzy_match_corrects() {
        let words = vec!["kubernetes".to_string()];
        // "kubernetis" is a common mishearing — 1 edit away
        let result = apply_custom_words("deploy to kubernetis", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "deploy to kubernetes");
    }

    #[test]
    fn phonetic_match_corrects() {
        let words = vec!["EchoType".to_string()];
        // Soundex similarity should help match "ekotype"
        let result = apply_custom_words("I use ekotype daily", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "I use EchoType daily");
    }

    #[test]
    fn ngram_match_corrects() {
        // "Charge Bee" said as two words should match "ChargeBee"
        let words = vec!["ChargeBee".to_string()];
        let result = apply_custom_words("I use Charge Bee for billing", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "I use ChargeBee for billing");
    }

    #[test]
    fn preserves_case_uppercase() {
        let words = vec!["kubernetes".to_string()];
        let result = apply_custom_words("KUBERNETIS is great", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "KUBERNETES is great");
    }

    #[test]
    fn preserves_case_title() {
        let words = vec!["kubernetes".to_string()];
        let result = apply_custom_words("Kubernetis is great", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "Kubernetes is great");
    }

    #[test]
    fn preserves_punctuation() {
        let words = vec!["EchoType".to_string()];
        let result = apply_custom_words("I love echotype!", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "I love EchoType!");
    }

    #[test]
    fn no_match_leaves_unchanged() {
        let words = vec!["kubernetes".to_string()];
        let result = apply_custom_words("the cat sat on the mat", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "the cat sat on the mat");
    }

    #[test]
    fn empty_words_returns_original() {
        let result = apply_custom_words("hello world", &[], DEFAULT_THRESHOLD);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn empty_text_returns_empty() {
        let words = vec!["test".to_string()];
        let result = apply_custom_words("", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "");
    }

    #[test]
    fn multiple_custom_words() {
        let words = vec!["EchoType".to_string(), "Kubernetes".to_string()];
        let result = apply_custom_words(
            "I use echotype with kubernetis",
            &words,
            DEFAULT_THRESHOLD,
        );
        assert_eq!(result, "I use EchoType with Kubernetes");
    }

    #[test]
    fn multi_word_custom_word() {
        let words = vec!["Visual Studio".to_string()];
        let result = apply_custom_words("open visualstudio now", &words, DEFAULT_THRESHOLD);
        assert_eq!(result, "open Visual Studio now");
    }
}
