use byte_unit::Byte;
use std::str::FromStr;

const MAX_S3_OBJECT_SIZE: u64 = 50 * 1024 * 1024 * 1024 * 1024; // 50 TiB

pub fn check_human_bytes(value: &str) -> Result<String, String> {
    let result = Byte::from_str(value).map_err(|e| e.to_string())?;
    let bytes: u64 = result
        .as_u128()
        .try_into()
        .map_err(|e: std::num::TryFromIntError| e.to_string())?;

    if bytes > MAX_S3_OBJECT_SIZE {
        return Err(format!(
            "value exceeds maximum S3 object size (50 TiB): {}",
            value
        ));
    }

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
    fn check_valid_value_without_limit() {
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
    fn check_invalid_value_without_limit() {
        assert!(check_human_bytes("524287a").is_err());
        assert!(check_human_bytes("5Zib").is_err());
    }

    #[test]
    fn check_exceeds_max_s3_object_size() {
        let result = check_human_bytes("51TiB");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("50 TiB"));

        let result = check_human_bytes("1PiB");
        assert!(result.is_err());
    }

    #[test]
    fn parse_valid_value_without_limit() {
        assert_eq!(8 * 1024 * 1024, parse_human_bytes("8MiB").unwrap());
        assert_eq!(5 * 1024 * 1024, parse_human_bytes("5242880").unwrap());
        assert_eq!(10 * 1024 * 1024 * 1024, parse_human_bytes("10GiB").unwrap());
    }

    #[test]
    fn parse_invalid_value_without_limit() {
        assert!(parse_human_bytes("524287a").is_err());
        assert!(parse_human_bytes("5Zib").is_err());
    }
}
