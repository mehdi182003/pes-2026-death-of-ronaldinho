//! Text helpers shared by the parsers.

/// Decodes bytes up to the first NUL. Names in Vice City's files are ASCII;
/// Latin-1 decoding never fails and keeps any other byte visible.
pub(crate) fn latin1_until_nul(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes[..end].iter().map(|&b| char::from(b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_at_the_first_nul_and_keeps_latin1() {
        assert_eq!(latin1_until_nul(b"player\0\0junk"), "player");
        assert_eq!(latin1_until_nul(b"Root"), "Root");
        assert_eq!(latin1_until_nul(&[0xE9, b't', b'\xE9']), "été");
    }
}
