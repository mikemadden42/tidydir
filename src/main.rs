use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

/// Directory (inside the target directory) that files are filed into.
const DEST_DIR: &str = "Documents";

/// Extensions that are left where they are instead of being filed away.
/// "9" was special-cased by the original version of this tool; it is kept
/// here so behavior doesn't change. Compared case-insensitively.
const SKIPPED_EXTENSIONS: &[&str] = &["9"];

const USAGE: &str = "Usage: tidydir [-n|--dry-run] [DIR]

Moves each regular file in DIR (default: your home directory) into
DIR/Documents/<extension>/. Dot files, files without an extension,
symlinks and directories are left alone. Existing files are never
overwritten.

Options:
  -n, --dry-run  Show what would be moved without moving anything
  -h, --help     Show this help";

#[derive(Debug, PartialEq)]
struct Move {
    from: PathBuf,
    to: PathBuf,
}

#[derive(Debug, Default)]
struct Plan {
    moves: Vec<Move>,
    /// Files that were deliberately left alone, with the reason.
    skipped: Vec<String>,
    /// Entries that couldn't be inspected.
    errors: Vec<String>,
}

/// Works out where each file in `dir` should go without changing anything.
/// Only failing to read `dir` itself is fatal; problems with individual
/// entries are collected so one bad entry doesn't stop the rest.
fn plan(dir: &Path) -> io::Result<Plan> {
    let mut plan = Plan::default();
    for entry in fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                plan.errors.push(format!("reading {}: {e}", dir.display()));
                continue;
            }
        };
        let path = entry.path();

        // file_type() doesn't follow symlinks, so dangling links can't fail
        // here, and links are skipped rather than moved.
        match entry.file_type() {
            Ok(file_type) if file_type.is_file() => {}
            Ok(_) => continue,
            Err(e) => {
                plan.errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        }

        let file_name = entry.file_name();
        if file_name.to_string_lossy().starts_with('.') {
            continue;
        }

        let ext = match path.extension() {
            Some(ext) => ext,
            None => continue,
        };
        let ext = match ext.to_str() {
            Some(ext) => ext.to_lowercase(),
            None => {
                plan.skipped
                    .push(format!("{}: extension is not valid UTF-8", path.display()));
                continue;
            }
        };
        if SKIPPED_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        let to = dir.join(DEST_DIR).join(&ext).join(&file_name);
        if to.exists() {
            plan.skipped.push(format!(
                "{}: {} already exists",
                path.display(),
                to.display()
            ));
            continue;
        }
        plan.moves.push(Move { from: path, to });
    }
    plan.moves.sort_by(|a, b| a.from.cmp(&b.from));
    Ok(plan)
}

/// Carries out `moves`, returning one message per move that failed.
fn apply(moves: &[Move]) -> Vec<String> {
    let mut errors = Vec::new();
    for m in moves {
        if let Err(e) = move_file(m) {
            errors.push(format!("moving {}: {e}", m.from.display()));
        } else {
            println!("Moved {} to {}", m.from.display(), m.to.display());
        }
    }
    errors
}

fn move_file(m: &Move) -> io::Result<()> {
    if let Some(parent) = m.to.parent() {
        fs::create_dir_all(parent)?;
    }
    // Check again in case the destination appeared since planning. There is
    // still a small window before rename(), which overwrites on most
    // platforms; that's acceptable for a single-user tidying tool.
    if m.to.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", m.to.display()),
        ));
    }
    fs::rename(&m.from, &m.to)
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

struct Args {
    dry_run: bool,
    dir: Option<PathBuf>,
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Args, String> {
    let mut parsed = Args {
        dry_run: false,
        dir: None,
    };
    for arg in args {
        match arg.to_str() {
            Some("-n" | "--dry-run") => parsed.dry_run = true,
            Some("-h" | "--help") => return Err(String::new()),
            Some(s) if s.starts_with('-') => return Err(format!("unknown option: {s}")),
            _ if parsed.dir.is_some() => return Err("too many arguments".to_string()),
            _ => parsed.dir = Some(PathBuf::from(arg)),
        }
    }
    Ok(parsed)
}

fn run(args: Args) -> Result<(), ()> {
    let dir = match args.dir.or_else(home_dir) {
        Some(dir) => dir,
        None => {
            eprintln!("Error: could not determine home directory; pass DIR explicitly");
            return Err(());
        }
    };

    let plan = match plan(&dir) {
        Ok(plan) => plan,
        Err(e) => {
            eprintln!("Error: reading {}: {e}", dir.display());
            return Err(());
        }
    };

    for msg in &plan.skipped {
        println!("Skipped {msg}");
    }

    let mut errors = plan.errors;
    if args.dry_run {
        for m in &plan.moves {
            println!("Would move {} to {}", m.from.display(), m.to.display());
        }
    } else {
        errors.extend(apply(&plan.moves));
    }

    for msg in &errors {
        eprintln!("Error: {msg}");
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(())
    }
}

fn main() {
    let args = match parse_args(env::args_os().skip(1)) {
        Ok(args) => args,
        Err(msg) if msg.is_empty() => {
            println!("{USAGE}");
            return;
        }
        Err(msg) => {
            eprintln!("Error: {msg}\n\n{USAGE}");
            process::exit(2);
        }
    };
    if run(args).is_err() {
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that is removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let path = env::temp_dir().join(format!("tidydir-test-{}-{name}", process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn touch(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, name).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn moves(dir: &TempDir) -> Vec<(String, String)> {
        let rel = |p: &Path| {
            p.strip_prefix(&dir.0)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        };
        plan(&dir.0)
            .unwrap()
            .moves
            .iter()
            .map(|m| (rel(&m.from), rel(&m.to)))
            .collect()
    }

    fn pair(from: &str, to: &str) -> (String, String) {
        (from.to_string(), to.to_string())
    }

    #[test]
    fn files_by_lowercased_extension() {
        let dir = TempDir::new("lowercase");
        dir.touch("a.txt");
        dir.touch("B.TXT");
        dir.touch("c.pdf");
        assert_eq!(
            moves(&dir),
            vec![
                pair("B.TXT", "Documents/txt/B.TXT"),
                pair("a.txt", "Documents/txt/a.txt"),
                pair("c.pdf", "Documents/pdf/c.pdf"),
            ]
        );
    }

    #[test]
    fn leaves_dotfiles_extensionless_skipped_and_dirs() {
        let dir = TempDir::new("leaves");
        dir.touch(".hidden");
        dir.touch(".config.toml");
        dir.touch("Makefile");
        dir.touch("page.9");
        dir.touch("PAGE.9");
        fs::create_dir(dir.0.join("folder.d")).unwrap();
        assert_eq!(moves(&dir), vec![]);
    }

    #[test]
    fn existing_destination_is_skipped_not_overwritten() {
        let dir = TempDir::new("exists");
        dir.touch("a.txt");
        let existing = dir.touch("Documents/txt/a.txt");
        let plan = plan(&dir.0).unwrap();
        assert_eq!(plan.moves, vec![]);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(fs::read_to_string(existing).unwrap(), "Documents/txt/a.txt");
    }

    #[test]
    fn apply_moves_files_and_creates_dirs() {
        let dir = TempDir::new("apply");
        dir.touch("a.txt");
        let plan = plan(&dir.0).unwrap();
        assert_eq!(apply(&plan.moves), Vec::<String>::new());
        assert!(!dir.0.join("a.txt").exists());
        assert!(dir.0.join("Documents/txt/a.txt").is_file());
    }

    #[test]
    fn apply_refuses_to_overwrite() {
        let dir = TempDir::new("race");
        dir.touch("a.txt");
        let plan = plan(&dir.0).unwrap();
        // Destination appears between planning and applying.
        dir.touch("Documents/txt/a.txt");
        assert_eq!(apply(&plan.moves).len(), 1);
        assert!(dir.0.join("a.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_does_not_stop_the_run() {
        let dir = TempDir::new("symlink");
        std::os::unix::fs::symlink(dir.0.join("missing"), dir.0.join("broken.lnk")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("a.txt"), dir.0.join("link.txt")).unwrap();
        dir.touch("a.txt");
        let plan = plan(&dir.0).unwrap();
        assert_eq!(plan.errors, Vec::<String>::new());
        assert_eq!(plan.moves.len(), 1);
    }

    #[test]
    fn missing_dir_is_an_error() {
        let dir = TempDir::new("missing");
        assert!(plan(&dir.0.join("nope")).is_err());
    }

    #[test]
    fn parses_args() {
        let parse = |args: &[&str]| parse_args(args.iter().map(OsString::from));
        let args = parse(&["-n", "some/dir"]).unwrap();
        assert!(args.dry_run);
        assert_eq!(args.dir, Some(PathBuf::from("some/dir")));
        assert!(parse(&[]).unwrap().dir.is_none());
        assert_eq!(parse(&["--help"]).err(), Some(String::new()));
        assert!(parse(&["--bogus"]).is_err());
        assert!(parse(&["a", "b"]).is_err());
    }
}
