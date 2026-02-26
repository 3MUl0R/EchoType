use rusqlite::Connection;
use tracing::{debug, info};

/// A pre-built replacement rule: a lowercased alias → correction.
struct Rule {
    alias_lower: String,
    correction: String,
}

/// Apply vocabulary corrections to text using the given collection.
///
/// Algorithm:
/// 1. Load all entries from the collection
/// 2. Flatten aliases into (alias_lower, correction) pairs
/// 3. Sort by alias length descending (longest-match-first)
/// 4. Single-pass, non-recursive scan: for each position, try the longest alias first
/// 5. Case-insensitive matching at word boundaries only
/// 6. Replaced text is not re-scanned (prevents infinite loops)
pub fn apply_corrections(conn: &Connection, collection_id: i64, text: &str) -> String {
    let entries = match crate::db::vocabulary::list_entries(conn, collection_id) {
        Ok(e) => e,
        Err(e) => {
            debug!(%e, "Failed to load vocabulary entries");
            return text.to_string();
        }
    };

    if entries.is_empty() {
        return text.to_string();
    }

    // Build rules: flatten all aliases into (alias_lower, correction)
    let mut rules: Vec<Rule> = Vec::new();
    for entry in &entries {
        for alias in &entry.aliases {
            let alias_trimmed = alias.trim();
            if !alias_trimmed.is_empty() {
                rules.push(Rule {
                    alias_lower: alias_trimmed.to_lowercase(),
                    correction: entry.correction.clone(),
                });
            }
        }
    }

    // Sort by alias length descending (longest-match-first)
    rules.sort_by(|a, b| b.alias_lower.len().cmp(&a.alias_lower.len()));

    if rules.is_empty() {
        return text.to_string();
    }

    let text_lower = text.to_lowercase();
    let text_chars: Vec<char> = text.chars().collect();
    let lower_chars: Vec<char> = text_lower.chars().collect();

    // Guard against to_lowercase() changing char count (e.g. German ß → ss)
    if text_chars.len() != lower_chars.len() {
        return text.to_string();
    }

    let len = text_chars.len();

    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let mut replacements = 0u32;

    while i < len {
        let mut matched = false;

        for rule in &rules {
            let alias_chars: Vec<char> = rule.alias_lower.chars().collect();
            let alias_len = alias_chars.len();

            if i + alias_len > len {
                continue;
            }

            // Check word boundary at start
            if i > 0 && is_word_char(lower_chars[i - 1]) {
                continue;
            }

            // Check word boundary at end
            if i + alias_len < len && is_word_char(lower_chars[i + alias_len]) {
                continue;
            }

            // Compare characters (case-insensitive — using pre-lowered)
            let slice = &lower_chars[i..i + alias_len];
            if slice == alias_chars.as_slice() {
                // Preserve case pattern from original text in the correction
                let corrected = transfer_case(&text_chars[i..i + alias_len], &rule.correction);
                result.push_str(&corrected);
                i += alias_len;
                matched = true;
                replacements += 1;
                break;
            }
        }

        if !matched {
            result.push(text_chars[i]);
            i += 1;
        }
    }

    if replacements > 0 {
        info!(replacements, "Applied vocabulary corrections");
    }

    result
}

/// Check if a character is a word character (alphanumeric or apostrophe).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

/// Transfer the case pattern from the original text to the correction.
///
/// - If all source chars are uppercase → UPPERCASE correction
/// - If first source char is uppercase, rest lower → Title Case correction
/// - Otherwise → correction as-is
fn transfer_case(source: &[char], correction: &str) -> String {
    if source.is_empty() {
        return correction.to_string();
    }

    let alpha_chars: Vec<char> = source
        .iter()
        .copied()
        .filter(|c| c.is_alphabetic())
        .collect();

    if alpha_chars.is_empty() {
        return correction.to_string();
    }

    let all_upper = alpha_chars.iter().all(|c| c.is_uppercase());
    let title_case = alpha_chars[0].is_uppercase()
        && alpha_chars.len() > 1
        && alpha_chars[1..].iter().all(|c| c.is_lowercase());

    if all_upper {
        correction.to_uppercase()
    } else if title_case {
        let mut chars = correction.chars();
        match chars.next() {
            Some(first) => {
                let mut result: String = first.to_uppercase().collect();
                result.extend(chars);
                result
            }
            None => correction.to_string(),
        }
    } else {
        correction.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::vocabulary;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn basic_replacement() {
        let conn = setup();
        let cid = vocabulary::create_collection(&conn, "test").unwrap();
        vocabulary::add_entry(&conn, cid, "EchoType", &["echo type".to_string()]).unwrap();

        let result = apply_corrections(&conn, cid, "I use echo type for dictation");
        assert_eq!(result, "I use EchoType for dictation");
    }

    #[test]
    fn case_insensitive_match() {
        let conn = setup();
        let cid = vocabulary::create_collection(&conn, "test").unwrap();
        vocabulary::add_entry(&conn, cid, "EchoType", &["echo type".to_string()]).unwrap();

        let result = apply_corrections(&conn, cid, "ECHO TYPE is great");
        assert_eq!(result, "ECHOTYPE is great");
    }

    #[test]
    fn title_case_transfer() {
        let conn = setup();
        let cid = vocabulary::create_collection(&conn, "test").unwrap();
        vocabulary::add_entry(&conn, cid, "kubernetes", &["cooper net ease".to_string()]).unwrap();

        let result = apply_corrections(&conn, cid, "Cooper net ease is a platform");
        assert_eq!(result, "Kubernetes is a platform");
    }

    #[test]
    fn word_boundary_respected() {
        let conn = setup();
        let cid = vocabulary::create_collection(&conn, "test").unwrap();
        vocabulary::add_entry(&conn, cid, "cat", &["kat".to_string()]).unwrap();

        // "kat" should NOT match inside "skate"
        let result = apply_corrections(&conn, cid, "I skate every day");
        assert_eq!(result, "I skate every day");

        // But standalone "kat" should match
        let result = apply_corrections(&conn, cid, "The kat sat down");
        assert_eq!(result, "The cat sat down");
    }

    #[test]
    fn longest_match_first() {
        let conn = setup();
        let cid = vocabulary::create_collection(&conn, "test").unwrap();
        vocabulary::add_entry(&conn, cid, "EchoType Pro", &["echo type pro".to_string()]).unwrap();
        vocabulary::add_entry(&conn, cid, "EchoType", &["echo type".to_string()]).unwrap();

        let result = apply_corrections(&conn, cid, "I use echo type pro daily");
        assert_eq!(result, "I use EchoType Pro daily");
    }

    #[test]
    fn multiple_aliases() {
        let conn = setup();
        let cid = vocabulary::create_collection(&conn, "test").unwrap();
        vocabulary::add_entry(
            &conn,
            cid,
            "EchoType",
            &[
                "echo type".to_string(),
                "eco type".to_string(),
                "ekko type".to_string(),
            ],
        )
        .unwrap();

        assert_eq!(
            apply_corrections(&conn, cid, "eco type is nice"),
            "EchoType is nice"
        );
        assert_eq!(
            apply_corrections(&conn, cid, "ekko type works"),
            "EchoType works"
        );
    }

    #[test]
    fn no_recursive_replacement() {
        let conn = setup();
        let cid = vocabulary::create_collection(&conn, "test").unwrap();
        // Create a circular-ish rule: foo → bar, bar → foo
        vocabulary::add_entry(&conn, cid, "bar", &["foo".to_string()]).unwrap();
        vocabulary::add_entry(&conn, cid, "foo", &["bar".to_string()]).unwrap();

        // "foo" should become "bar", but "bar" should NOT be re-scanned
        let result = apply_corrections(&conn, cid, "foo bar");
        assert_eq!(result, "bar foo");
    }

    #[test]
    fn empty_collection() {
        let conn = setup();
        let cid = vocabulary::create_collection(&conn, "test").unwrap();
        let result = apply_corrections(&conn, cid, "some text");
        assert_eq!(result, "some text");
    }

    #[test]
    fn transfer_case_variations() {
        assert_eq!(transfer_case(&['H', 'e', 'l', 'l', 'o'], "world"), "World");
        assert_eq!(transfer_case(&['H', 'E', 'L', 'L', 'O'], "world"), "WORLD");
        assert_eq!(transfer_case(&['h', 'e', 'l', 'l', 'o'], "World"), "World");
    }
}
