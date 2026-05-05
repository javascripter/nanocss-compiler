fn to_base36(mut value: u32) -> String {
    if value == 0 {
        return "0".to_string();
    }

    let mut result = Vec::new();
    while value > 0 {
        let digit = value % 36;
        result.push(match digit {
            0..=9 => (b'0' + digit as u8) as char,
            _ => (b'a' + (digit as u8 - 10)) as char,
        });
        value /= 36;
    }
    result.iter().rev().collect()
}

pub(crate) fn hash(value: &str) -> String {
    let mut hash_value = 0_u32;
    for character in value.encode_utf16() {
        hash_value = hash_value
            .wrapping_shl(5)
            .wrapping_sub(hash_value)
            .wrapping_add(character as u32)
            & 0x7fff_ffff;
    }
    to_base36(hash_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_existing_compiler_hashes() {
        assert_eq!(
            hash(r#"{"0%":{"opacity":0},"100%":{"opacity":1}}"#),
            "1ii5yk"
        );
    }
}
