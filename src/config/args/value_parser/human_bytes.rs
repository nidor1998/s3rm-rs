use byte_unit::Byte;
use std::str::FromStr;

pub fn check_human_bytes(value: &str) -> Result<String, String> {
    let result = Byte::from_str(value).map_err(|e| e.to_string())?;
    TryInto::<u64>::try_into(result.as_u128()).map_err(|e| e.to_string())?;

    Ok(value.to_string())
}

pub fn parse_human_bytes(value: &str) -> Result<u64, String> {
    check_human_bytes(value)?;

    let result = Byte::from_str(value).map_err(|e| e.to_string())?;
    Ok(result
        .as_u128()
        .try_into()
        .expect("check_human_bytes already validated u64 range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_valid_value() {
        check_human_bytes("0").unwrap();
        check_human_bytes("1KiB").unwrap();
        check_human_bytes("1KB").unwrap();
        check_human_bytes("1024").unwrap();
        check_human_bytes("5MiB").unwrap();
        check_human_bytes("5242880").unwrap();
        check_human_bytes("5GiB").unwrap();
        check_human_bytes("8MiB").unwrap();
        check_human_bytes("10GiB").unwrap();
        check_human_bytes("1TiB").unwrap();
        check_human_bytes("50TiB").unwrap();
    }

    #[test]
    fn check_invalid_value() {
        assert!(check_human_bytes("524287a").is_err());
        assert!(check_human_bytes("5Zib").is_err());
    }

    #[test]
    fn parse_valid_value() {
        assert_eq!(8 * 1024 * 1024, parse_human_bytes("8MiB").unwrap());
        assert_eq!(5 * 1024 * 1024, parse_human_bytes("5242880").unwrap());
        assert_eq!(10 * 1024 * 1024 * 1024, parse_human_bytes("10GiB").unwrap());
    }

    #[test]
    fn parse_invalid_value() {
        assert!(parse_human_bytes("524287a").is_err());
        assert!(parse_human_bytes("5Zib").is_err());
    }

    #[test]
    fn parse_lowercase_b_suffix_as_bits() {
        // The last letter of the suffix is case-sensitive: "B" is bytes, "b" is bits.
        assert_eq!(8_000_000, parse_human_bytes("8MB").unwrap());
        assert_eq!(1_000_000, parse_human_bytes("8Mb").unwrap());
        assert_eq!(1_000_000, parse_human_bytes("8mb").unwrap());
        assert_eq!(8 * 1024 * 1024, parse_human_bytes("8MiB").unwrap());
        assert_eq!(1024 * 1024, parse_human_bytes("8Mib").unwrap());
        assert_eq!(671_088_640, parse_human_bytes("5gib").unwrap());
    }

    #[test]
    fn check_accepts_lowercase_b_suffix() {
        // Bits are accepted, not rejected: the value is just one eighth of the byte value.
        assert_eq!(check_human_bytes("8Mb").unwrap(), "8Mb");
        assert_eq!(check_human_bytes("8mb").unwrap(), "8mb");
        assert_eq!(check_human_bytes("5gib").unwrap(), "5gib");
    }

    #[test]
    fn parse_readme_size_suffix_table() {
        // The values of the "Size suffixes" table in README.md.
        let cases = [
            ("8MiB", 8_388_608),
            ("8MB", 8_000_000),
            ("8Mib", 1_048_576),
            ("8mb", 1_000_000),
            ("5GiB", 5_368_709_120),
            ("5gib", 671_088_640),
            // "--filter-larger-size 100mb matches objects of 12.5MB or larger"
            ("100mb", 12_500_000),
        ];

        for (value, expected_bytes) in cases {
            assert_eq!(
                parse_human_bytes(value).unwrap(),
                expected_bytes,
                "value: {value}"
            );
        }
    }

    #[test]
    fn parse_zero() {
        assert_eq!(0, parse_human_bytes("0").unwrap());
    }

    #[test]
    fn parse_one_byte() {
        assert_eq!(1, parse_human_bytes("1").unwrap());
    }

    #[test]
    fn parse_max_u64_rejects_overflow() {
        // Values exceeding u64::MAX should be rejected by check_human_bytes
        assert!(parse_human_bytes("18446744073709551616").is_err());
    }

    #[test]
    fn check_rejects_overflow() {
        // u64::MAX + 1 = 18446744073709551616
        assert!(check_human_bytes("18446744073709551616").is_err());
    }

    #[test]
    fn check_accepts_large_valid_value() {
        // 50 TiB is within u64 range
        assert!(check_human_bytes("50TiB").is_ok());
    }
}
