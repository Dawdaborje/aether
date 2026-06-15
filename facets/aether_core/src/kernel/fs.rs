use std::fs;
use std::io::{Read, Write};
use std::path::Path;

pub fn create_file(file_path: &str, content: Option<String>) -> std::io::Result<()> {
    let mut file = fs::File::create_new(file_path).expect("Could not create file");
    file.write(content.expect("Failed").as_bytes()).expect("Could not write to file");
    Ok(())
}

pub fn delete_file(file_path: &str) -> std::io::Result<()> {
    fs::remove_file(file_path).expect("Could not delete file");
    Ok(())
}

pub fn open_file(file_path: &str) -> std::io::Result<String> {
    let mut file = fs::File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}