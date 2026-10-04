use std::fmt;

use super::error::{Error, Result};

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const BYTES: usize = 20;
const CHARS: usize = 32;
const GROUP: usize = 4;

/// 160-bit random code that wraps the age identity a second time (`keys/recovery.age`).
/// Random already, so it is used as the scrypt passphrase as is, without a strength check.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryCode(String);

impl RecoveryCode {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; BYTES];
        getrandom::fill(&mut bytes).map_err(|e| Error::Encrypt(e.to_string()))?;
        let mut out = String::with_capacity(CHARS);
        let (mut acc, mut bits) = (0u32, 0u32);
        for byte in bytes {
            acc = (acc << 8) | u32::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
            }
            acc &= (1 << bits) - 1;
        }
        Ok(Self(out))
    }

    /// Accepts what a user pastes back: dashes, spaces and lowercase are ignored.
    pub fn parse(input: &str) -> Result<Self> {
        let canonical: String = input.chars().filter(|c| !c.is_whitespace() && *c != '-').map(|c| c.to_ascii_uppercase()).collect();
        if canonical.len() == CHARS && canonical.bytes().all(|b| ALPHABET.contains(&b)) {
            Ok(Self(canonical))
        } else {
            Err(Error::InvalidRecoveryCode)
        }
    }

    /// The 32 characters without separators: what the identity is actually wrapped with.
    pub fn canonical(&self) -> &str {
        &self.0
    }
}

/// `ABCD-EFGH-…`, eight groups of four.
impl fmt::Display for RecoveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, chunk) in self.0.as_bytes().chunks(GROUP).enumerate() {
            if i > 0 {
                f.write_str("-")?;
            }
            f.write_str(std::str::from_utf8(chunk).expect("ascii alphabet"))?;
        }
        Ok(())
    }
}

/// Never prints the secret, so a stray `{:?}` cannot leak it into a log.
impl fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCode(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_is_eight_groups_of_four_that_parse_back() {
        let code = RecoveryCode::generate().unwrap();
        let shown = code.to_string();
        assert_eq!(shown.len(), CHARS + 7);
        assert!(shown.split('-').all(|g| g.len() == GROUP));
        assert_eq!(RecoveryCode::parse(&shown).unwrap(), code);
        assert_eq!(code.canonical().len(), CHARS);
    }

    #[test]
    fn two_codes_differ() {
        assert_ne!(RecoveryCode::generate().unwrap(), RecoveryCode::generate().unwrap());
    }

    #[test]
    fn parse_ignores_case_dashes_and_whitespace() {
        let code = RecoveryCode::generate().unwrap();
        let messy = format!(" {} \n", code.to_string().to_lowercase().replace('-', " - "));
        assert_eq!(RecoveryCode::parse(&messy).unwrap(), code);
    }

    #[test]
    fn parse_rejects_wrong_length_and_characters() {
        let code = RecoveryCode::generate().unwrap();
        let short = &code.canonical()[..CHARS - 1];
        assert!(matches!(RecoveryCode::parse(short), Err(Error::InvalidRecoveryCode)));
        assert!(matches!(RecoveryCode::parse(&format!("{}A", code.canonical())), Err(Error::InvalidRecoveryCode)));
        // 0, 1, 8 and 9 are not in the alphabet.
        let bad = format!("0189{}", &code.canonical()[4..]);
        assert!(matches!(RecoveryCode::parse(&bad), Err(Error::InvalidRecoveryCode)));
        assert!(matches!(RecoveryCode::parse(""), Err(Error::InvalidRecoveryCode)));
        assert!(matches!(RecoveryCode::parse(&"é".repeat(16)), Err(Error::InvalidRecoveryCode)));
    }

    #[test]
    fn debug_hides_the_secret() {
        let code = RecoveryCode::generate().unwrap();
        assert!(!format!("{code:?}").contains(&code.canonical()[..8]));
    }
}
