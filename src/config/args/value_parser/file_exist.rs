use std::path::PathBuf;

pub fn is_file_exist(file_path: &str) -> Result<String, String> {
    let file_path = PathBuf::from(file_path);

    if file_path.exists() && file_path.is_file() {
        Ok(file_path.to_string_lossy().to_string())
    } else {
        Err(format!("file not found: {}", file_path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_existing_file_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.lua");
        fs::write(&file_path, "return true").unwrap();

        let result = is_file_exist(file_path.to_str().unwrap());
        assert!(result.is_ok());
        let returned = PathBuf::from(result.unwrap());
        assert_eq!(returned, file_path);
    }

    #[test]
    fn test_nonexistent_file_returns_err() {
        let result = is_file_exist("/nonexistent/path/to/script.lua");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("file not found"));
    }

    #[test]
    fn test_directory_returns_err() {
        let dir = tempfile::tempdir().unwrap();

        let result = is_file_exist(dir.path().to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("file not found"));
    }

    #[test]
    fn test_relative_path_works() {
        // Cargo.toml exists at the project root and tests run from there
        let result = is_file_exist("Cargo.toml");
        assert!(result.is_ok());
    }

    #[test]
    fn test_path_with_dot_components() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        let file_path = sub.join("test.lua");
        fs::write(&file_path, "return true").unwrap();

        // Use ./sub/../sub/test.lua style path
        let dotdot_path = format!("{}/sub/../sub/test.lua", dir.path().to_str().unwrap());
        let result = is_file_exist(&dotdot_path);
        assert!(result.is_ok());
    }
}
