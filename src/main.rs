use std::fs;
use std::io;
use std::path::Path;

fn clean() -> io::Result<()> {
    let current_dir = Path::new(".");
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::metadata(&path)?;

        if metadata.is_file() {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Skip dotfiles
            if !file_name_str.starts_with('.') {
                if let Some(extension) = path.extension() {
                    if let Some(ext_str) = extension.to_str() {
                        if ext_str != "9" {
                            let dest_dir = Path::new("Documents").join(ext_str);

                            fs::create_dir_all(&dest_dir)?;

                            let dest_path = dest_dir.join(&file_name);
                            if dest_path.exists() {
                                println!("File {path:?} already exists in {dest_dir:?}");
                            } else {
                                fs::rename(&path, &dest_path)?;
                                println!("Moved {path:?} to {dest_dir:?}");
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = clean() {
        eprintln!("Error: {e}");
    }
}
