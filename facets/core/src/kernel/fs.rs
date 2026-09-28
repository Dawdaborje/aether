use std::fs;
use std::io::{Read, Write};

pub fn create_file(file_path: &str, content: Option<String>) -> std::io::Result<()> {
    let content = content.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "file content is required")
    })?;
    let mut file = fs::File::create_new(file_path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

pub fn delete_file(file_path: &str) -> std::io::Result<()> {
    fs::remove_file(file_path)
}

pub fn open_file(file_path: &str) -> std::io::Result<String> {
    let mut file = fs::File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}
