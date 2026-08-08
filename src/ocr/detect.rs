//! Finding sensitive strings in OCR output.
//!
//! Card numbers get a Luhn check rather than a bare length test. Without it any
//! 16-digit run — order numbers, timestamps, IDs — would be redacted, and a
//! redaction box that appears over the wrong thing is worse than none at all.

use crate::i18n::t;

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use super::Word;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Secret {
    Email,
    CreditCard,
    IpAddress,
    ApiKey,
    Phone,
}

impl Secret {
    pub fn label(self) -> &'static str {
        match self {
            Secret::Email => "Email",
            Secret::CreditCard => t("Card"),
            Secret::IpAddress => "IP",
            Secret::ApiKey => "API key",
            Secret::Phone => t("Phone number"),
        }
    }
}

/// A match, as a run of consecutive word indices — card numbers routinely span
/// four OCR "words".
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub kind: Secret,
    pub words: Vec<usize>,
}

macro_rules! re {
    ($name:ident, $pattern:literal) => {
        static $name: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new($pattern).expect("static pattern compiles"));
    };
}

re!(EMAIL, r"(?i)[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}");
re!(IPV4, r"\b(?:\d{1,3}\.){3}\d{1,3}\b");
re!(IPV6, r"(?i)\b(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{1,4}\b");
re!(
    API_KEY,
    r"(?x)
      sk-[A-Za-z0-9_\-]{16,}
    | ghp_[A-Za-z0-9]{20,}
    | gho_[A-Za-z0-9]{20,}
    | github_pat_[A-Za-z0-9_]{20,}
    | AKIA[0-9A-Z]{16}
    | xox[baprs]-[A-Za-z0-9\-]{10,}
    | AIza[0-9A-Za-z_\-]{30,}
    | eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}
    "
);
// Deliberately narrow: a bare run of digits is far too common on a screen to
// treat as a phone number.
re!(
    PHONE,
    r"(?:\+\d{1,3}[\s.\-]?)?0?\d{2,4}[\s.\-]\d{3,4}[\s.\-]\d{3,4}"
);

pub fn classify_word(text: &str) -> Option<Secret> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if EMAIL.is_match(t) {
        return Some(Secret::Email);
    }
    if API_KEY.is_match(t) {
        return Some(Secret::ApiKey);
    }
    if IPV4.is_match(t) && valid_ipv4(t) {
        return Some(Secret::IpAddress);
    }
    if IPV6.is_match(t) {
        return Some(Secret::IpAddress);
    }
    if is_card_number(t) {
        return Some(Secret::CreditCard);
    }
    None
}

fn valid_ipv4(text: &str) -> bool {
    IPV4.find_iter(text).any(|m| {
        m.as_str()
            .split('.')
            .all(|o| o.parse::<u16>().is_ok_and(|v| v <= 255))
    })
}

/// Luhn check over the digits of `text`, requiring a plausible card length.
pub fn is_card_number(text: &str) -> bool {
    let digits: Vec<u32> = text.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    // Reject if the string carries anything that is not a digit or a separator:
    // "2024-01-02T03" should not be a card.
    if text
        .chars()
        .any(|c| !c.is_ascii_digit() && !matches!(c, ' ' | '-' | '.'))
    {
        return false;
    }
    luhn(&digits)
}

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let doubled = d * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Two words sit on the same line if their vertical spans mostly overlap.
fn same_line(a: &Word, b: &Word) -> bool {
    let overlap = (a.rect[3].min(b.rect[3]) - a.rect[1].max(b.rect[1])).max(0.0);
    let shortest = a.height().min(b.height()).max(1.0);
    overlap / shortest > 0.5
}

/// Scan recognised words for anything sensitive.
///
/// Single words are classified directly; card numbers are additionally sought
/// across runs of up to four adjacent words on the same line, because OCR
/// splits `4111 1111 1111 1111` into four.
pub fn scan(words: &[Word]) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut claimed = vec![false; words.len()];

    // Multi-word card numbers first: they should win over any per-word match.
    for start in 0..words.len() {
        for len in (2..=4).rev() {
            let end = start + len;
            if end > words.len() {
                continue;
            }
            let run = &words[start..end];
            if run.iter().any(|w| {
                w.text
                    .chars()
                    .any(|c| !c.is_ascii_digit() && !matches!(c, ' ' | '-' | '.'))
            }) {
                continue;
            }
            if !run.windows(2).all(|p| same_line(&p[0], &p[1])) {
                continue;
            }
            if claimed[start..end].iter().any(|c| *c) {
                continue;
            }
            let joined: String = run.iter().map(|w| w.text.trim()).collect();
            if is_card_number(&joined) {
                claimed[start..end].fill(true);
                findings.push(Finding {
                    kind: Secret::CreditCard,
                    words: (start..end).collect(),
                });
            }
        }
    }

    for (i, word) in words.iter().enumerate() {
        if claimed[i] {
            continue;
        }
        if let Some(kind) = classify_word(&word.text) {
            claimed[i] = true;
            findings.push(Finding {
                kind,
                words: vec![i],
            });
        }
    }

    findings
}

/// Phone numbers are scanned separately because the pattern is loose enough to
/// produce false positives; the UI keeps it behind its own toggle.
pub fn scan_phones(words: &[Word]) -> Vec<Finding> {
    words
        .iter()
        .enumerate()
        .filter(|(_, w)| PHONE.is_match(w.text.trim()) && !EMAIL.is_match(w.text.trim()))
        .map(|(i, _)| Finding {
            kind: Secret::Phone,
            words: vec![i],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, x: f32, y: f32) -> Word {
        Word {
            text: text.into(),
            rect: [x, y, x + 40.0, y + 12.0],
        }
    }

    #[test]
    fn emails_are_found() {
        assert_eq!(classify_word("tony@somewhere.com"), Some(Secret::Email));
        assert_eq!(
            classify_word("dummy-email.data+tag@example.co.uk"),
            Some(Secret::Email)
        );
        assert_eq!(classify_word("not-an-email"), None);
        assert_eq!(classify_word("a@b"), None, "needs a real TLD");
    }

    #[test]
    fn api_keys_are_found() {
        for key in [
            "sk-abcdefghijklmnopqrstuvwx",
            "ghp_abcdefghijklmnopqrstuvwxyz01",
            "AKIAIOSFODNN7EXAMPLE",
            "xoxb-123456789012-abcdefgh",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27u",
        ] {
            assert_eq!(classify_word(key), Some(Secret::ApiKey), "missed {key}");
        }
        assert_eq!(classify_word("sk-short"), None);
    }

    #[test]
    fn ip_addresses_are_validated_not_just_matched() {
        assert_eq!(classify_word("192.168.1.24"), Some(Secret::IpAddress));
        assert_eq!(
            classify_word("999.1.1.1"),
            None,
            "octets above 255 are not addresses"
        );
        assert_eq!(
            classify_word("2001:0db8:85a3:0000:0000:8a2e:0370:7334"),
            Some(Secret::IpAddress)
        );
    }

    #[test]
    fn luhn_separates_real_card_numbers_from_lookalikes() {
        // Standard published test numbers.
        assert!(is_card_number("4111111111111111"));
        assert!(is_card_number("5500 0000 0000 0004"));
        assert!(is_card_number("3782 822463 10005"));

        // Same length, fails the checksum — this is the case a length-only
        // test would wrongly redact.
        assert!(!is_card_number("4111111111111112"));
        assert!(!is_card_number("1234567890123456"));
    }

    #[test]
    fn card_detection_rejects_wrong_lengths_and_stray_characters() {
        assert!(!is_card_number("41111111111"), "too short");
        assert!(!is_card_number("41111111111111111111"), "too long");
        assert!(!is_card_number("order-4111111111111111"));
    }

    #[test]
    fn a_card_split_across_four_words_is_still_found() {
        let words = vec![
            w("4111", 0.0, 100.0),
            w("1111", 50.0, 100.0),
            w("1111", 100.0, 100.0),
            w("1111", 150.0, 100.0),
        ];
        let findings = scan(&words);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, Secret::CreditCard);
        assert_eq!(findings[0].words, vec![0, 1, 2, 3]);
    }

    #[test]
    fn digits_on_different_lines_are_not_joined_into_a_card() {
        let words = vec![
            w("4111", 0.0, 100.0),
            w("1111", 50.0, 100.0),
            // Far below: a different row of a table.
            w("1111", 100.0, 400.0),
            w("1111", 150.0, 400.0),
        ];
        assert!(
            scan(&words).iter().all(|f| f.kind != Secret::CreditCard),
            "words from different lines must not be concatenated"
        );
    }

    #[test]
    fn scan_finds_a_mixed_bag_without_double_claiming_words() {
        let words = vec![
            w("Email", 0.0, 0.0),
            w("real-email@gmail.com", 50.0, 0.0),
            w("192.168.0.1", 0.0, 40.0),
            w("4111", 0.0, 80.0),
            w("1111", 50.0, 80.0),
            w("1111", 100.0, 80.0),
            w("1111", 150.0, 80.0),
        ];
        let findings = scan(&words);

        let kinds: Vec<Secret> = findings.iter().map(|f| f.kind).collect();
        assert!(kinds.contains(&Secret::Email));
        assert!(kinds.contains(&Secret::IpAddress));
        assert!(kinds.contains(&Secret::CreditCard));

        // No word may belong to two findings.
        let mut seen = std::collections::HashSet::new();
        for f in &findings {
            for i in &f.words {
                assert!(seen.insert(*i), "word {i} claimed twice");
            }
        }
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        let words = vec![
            w("Xnapper", 0.0, 0.0),
            w("(Beta)", 50.0, 0.0),
            w("20", 100.0, 0.0),
            w("Aug", 130.0, 0.0),
            w("2022", 170.0, 0.0),
            w("$15", 220.0, 0.0),
        ];
        assert!(scan(&words).is_empty(), "{:?}", scan(&words));
    }

    #[test]
    fn phones_are_scanned_separately_and_skip_emails() {
        let words = vec![w("0912 345 678", 0.0, 0.0), w("a@b.com", 100.0, 0.0)];
        let phones = scan_phones(&words);
        assert_eq!(phones.len(), 1);
        assert_eq!(phones[0].words, vec![0]);
    }

    #[test]
    fn scanning_nothing_returns_nothing() {
        assert!(scan(&[]).is_empty());
        assert!(scan_phones(&[]).is_empty());
    }
}
