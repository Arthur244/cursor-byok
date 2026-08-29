pub(crate) fn parse_token_count(value: &str) -> Option<u64> {
    let value = value.trim().to_ascii_lowercase();
    let (number, multiplier) = match value.chars().last()? {
        'k' => (&value[..value.len() - 1], 1_000),
        'm' => (&value[..value.len() - 1], 1_000_000),
        _ => (value.as_str(), 1),
    };
    number.parse::<u64>().ok()?.checked_mul(multiplier)
}

pub(crate) fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 && tokens.is_multiple_of(1_000_000) {
        format!("{}M", tokens / 1_000_000)
    } else if tokens >= 1_000 && tokens.is_multiple_of(1_000) {
        format!("{}K", tokens / 1_000)
    } else {
        tokens.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_counts_parse_plain_and_abbreviated_values() {
        assert_eq!(parse_token_count("272000"), Some(272_000));
        assert_eq!(parse_token_count("272K"), Some(272_000));
        assert_eq!(parse_token_count("1m"), Some(1_000_000));
        assert_eq!(parse_token_count("invalid"), None);
    }

    #[test]
    fn token_counts_format_exact_thousands_and_millions() {
        assert_eq!(format_token_count(272_000), "272K");
        assert_eq!(format_token_count(1_000_000), "1M");
        assert_eq!(format_token_count(272_001), "272001");
    }
}
