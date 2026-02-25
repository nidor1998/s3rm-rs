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
    Ok(result.as_u128().try_into().unwrap())
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
}
