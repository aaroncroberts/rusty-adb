//! Android `ls -la` output parser — pure, async-free, fully unit-tested.
//!
//! Parsing is kept separate from I/O so every production code path can be
//! verified with plain unit tests (no ADB daemon required).

use std::path::PathBuf;

// ─── Android Filesystem Entry ──────────────────────────────────────────────────

/// A single entry returned by `adb shell ls -la`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidEntry {
    /// File or directory name (no path prefix)
    pub name: String,
    /// Full absolute path on the device
    pub path: PathBuf,
    /// Byte size (0 for directories — ls reports block size, not meaningful)
    pub size: u64,
    /// Pre-formatted modified date "YYYY-MM-DD" (or "--" if unknown)
    pub modified: String,
    /// `true` when the entry's mode starts with `d`
    pub is_dir: bool,
    /// `true` when the entry's mode starts with `l` (symlink)
    pub is_symlink: bool,
    /// `true` when the filename starts with `.`
    pub is_hidden: bool,
}

impl AndroidEntry {
    /// Human-readable file size; directories always show "--".
    pub fn size_display(&self) -> String {
        if self.is_dir {
            "--".to_string()
        } else {
            format_size(self.size)
        }
    }
}

// ─── Size formatting ───────────────────────────────────────────────────────────

pub(crate) fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ─── Parser ────────────────────────────────────────────────────────────────────

/// Parse the output of `adb shell ls -la <dir>` (Android Toybox format).
///
/// Example line:
/// ```text
/// drwxrwxrwx 17 root   sdcard_rw 3452 2024-01-15 12:00 DCIM
/// lrwxrwxrwx  1 root   sdcard_rw   21 2024-01-01 00:00 sdcard0 -> /storage/emulated/0
/// ```
pub fn parse_ls_output(output: &str, parent: &std::path::Path) -> Vec<AndroidEntry> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let result = parse_ls_line(trimmed, parent);
            // Warn on non-empty, non-header lines that we couldn't parse —
            // this helps diagnose OEM-specific ls format variations.
            if result.is_none()
                && !trimmed.is_empty()
                && !trimmed.starts_with("total ")
                && !trimmed.contains("Permission denied")
                && !trimmed.contains("No such file")
                && trimmed != "."
                && trimmed != ".."
            {
                // Only warn for lines that look like they should be entries
                // (start with a permission character: -, d, l, c, b, p, s)
                if matches!(
                    trimmed.chars().next(),
                    Some('-' | 'd' | 'l' | 'c' | 'b' | 'p' | 's')
                ) {
                    tracing::warn!(
                        line = %trimmed,
                        parent = %parent.display(),
                        "unrecognised adb ls line — may be an OEM ls format variation"
                    );
                }
            }
            result
        })
        .collect()
}

fn parse_ls_line(line: &str, parent: &std::path::Path) -> Option<AndroidEntry> {
    // Skip blank lines and the "total N" header
    if line.is_empty() || line.starts_with("total ") {
        return None;
    }
    // Skip lines that are error messages
    if line.contains("Permission denied") || line.contains("No such file") {
        return None;
    }

    let perms_char = line.chars().next()?;
    let is_dir = perms_char == 'd';
    let is_symlink = perms_char == 'l';

    // Tokenise: perms nlinks owner group size date time name...
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 8 {
        return None;
    }

    // Find the date token — always "YYYY-MM-DD" (10 chars, '-' at 4 and 7)
    let date_idx = tokens.iter().position(|t| {
        t.len() == 10 && t.as_bytes().get(4) == Some(&b'-') && t.as_bytes().get(7) == Some(&b'-')
    })?;

    if date_idx < 1 || date_idx + 2 >= tokens.len() {
        return None;
    }

    let size: u64 = tokens[date_idx - 1].parse().ok()?;
    let date_str = tokens[date_idx]; // "2024-01-15"
    let time_str = tokens[date_idx + 1]; // "12:00"
    let modified = format!("{} {}", date_str, &time_str[..5]); // "2024-01-15 12:00"

    // The name is everything after the time token in the original line.
    // We find the time token's offset carefully (it appears after the date).
    let date_offset = line.find(date_str)?;
    let after_date = &line[date_offset + date_str.len()..];
    let time_offset_in_after = after_date.find(time_str)?;
    let after_time = after_date[time_offset_in_after + time_str.len()..].trim();

    // Symlinks: "name -> /path/to/target" — strip the link target
    let name_raw = if is_symlink {
        after_time.split(" -> ").next().unwrap_or(after_time)
    } else {
        after_time
    };
    let name = name_raw.trim();

    // Skip self and parent directory entries
    if name == "." || name == ".." || name.is_empty() {
        return None;
    }

    Some(AndroidEntry {
        path: parent.join(name),
        is_dir,
        is_symlink,
        is_hidden: name.starts_with('.'),
        size,
        modified: modified[..10].to_string(), // keep just "YYYY-MM-DD"
        name: name.to_string(),
    })
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LS: &str = "\
total 48
drwxrwxrwx 17 root   sdcard_rw 3452 2024-01-15 12:00 .
drwxr-x---  5 root   root      4096 2024-01-01 00:00 ..
drwxrwxrwx  2 root   sdcard_rw 4096 2024-01-10 08:30 DCIM
drwxrwxrwx  2 root   sdcard_rw 4096 2024-01-14 09:15 Download
lrwxrwxrwx  1 root   sdcard_rw   21 2024-01-01 00:00 sdcard0 -> /storage/emulated/0
-rw-rw----  1 root   sdcard_rw 1234 2024-01-15 10:00 notes.txt
";

    // ── Directory / file / symlink parsing ────────────────────────────────────

    #[test]
    fn ls_parses_directories() {
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(SAMPLE_LS, parent);
        let dcim = entries.iter().find(|e| e.name == "DCIM").unwrap();
        assert!(dcim.is_dir);
        assert_eq!(dcim.path, parent.join("DCIM"));
        assert_eq!(dcim.modified, "2024-01-10");
    }

    #[test]
    fn ls_parses_files() {
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(SAMPLE_LS, parent);
        let f = entries.iter().find(|e| e.name == "notes.txt").unwrap();
        assert!(!f.is_dir);
        assert_eq!(f.size, 1234);
        assert_eq!(f.size_display(), "1 KB");
    }

    #[test]
    fn ls_parses_symlinks_strips_target() {
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(SAMPLE_LS, parent);
        let link = entries.iter().find(|e| e.name == "sdcard0").unwrap();
        assert!(link.is_symlink);
        assert_eq!(link.name, "sdcard0");
        // path should not include " -> /storage/emulated/0"
        assert_eq!(link.path, parent.join("sdcard0"));
    }

    #[test]
    fn ls_skips_dot_entries() {
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(SAMPLE_LS, parent);
        assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
    }

    #[test]
    fn ls_correct_entry_count() {
        let parent = std::path::Path::new("/sdcard");
        // Should have: DCIM, Download, sdcard0, notes.txt → 4
        let entries = parse_ls_output(SAMPLE_LS, parent);
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn ls_hidden_file_flagged() {
        let input = "-rw-rw----  1 root sdcard_rw 100 2024-01-15 10:00 .nomedia\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_hidden);
    }

    #[test]
    fn ls_empty_output() {
        let entries = parse_ls_output("total 0\n", std::path::Path::new("/sdcard"));
        assert!(entries.is_empty());
    }

    #[test]
    fn ls_permission_denied_line_skipped() {
        let input = "ls: /sdcard/private: Permission denied\n\
                     -rw-rw----  1 root sdcard_rw 100 2024-01-15 10:00 notes.txt\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "notes.txt");
    }

    #[test]
    fn ls_file_with_spaces_in_name() {
        let input = "-rw-rw----  1 root sdcard_rw 1234 2024-01-15 10:00 my vacation photos.jpg\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "my vacation photos.jpg");
    }

    // ── Additional edge-case tests ────────────────────────────────────────────

    #[test]
    fn ls_file_without_extension() {
        let input = "-rw-rw----  1 root sdcard_rw 42 2024-06-01 09:00 Makefile\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Makefile");
        assert!(!entries[0].is_dir);
    }

    #[test]
    fn ls_hidden_dir_flagged() {
        let input = "drwxrwxrwx  2 root sdcard_rw 4096 2024-01-15 10:00 .thumbnails\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_hidden);
        assert!(entries[0].is_dir);
    }

    #[test]
    fn ls_total_header_skipped() {
        let input = "total 1234\n-rw-rw----  1 root sdcard_rw 10 2024-01-15 10:00 a.txt\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        // Only the file, not the "total" line
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn ls_no_such_file_line_skipped() {
        let input = "/sdcard/missing: No such file or directory\n\
                     -rw-rw----  1 root sdcard_rw 10 2024-01-15 10:00 present.txt\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "present.txt");
    }

    #[test]
    fn ls_symlink_is_navigable() {
        let input = "lrwxrwxrwx  1 root sdcard_rw 21 2024-01-01 00:00 sdcard0 -> /storage/emulated/0\n";
        let entries = parse_ls_output(input, std::path::Path::new("/"));
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_symlink);
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].name, "sdcard0");
    }

    #[test]
    fn ls_path_joins_correctly() {
        let input = "drwxrwxrwx  2 root sdcard_rw 4096 2024-01-10 08:30 DCIM\n";
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(input, parent);
        assert_eq!(entries[0].path, std::path::PathBuf::from("/sdcard/DCIM"));
    }

    // ── Size formatting ───────────────────────────────────────────────────────

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
    }

    #[test]
    fn format_size_kb() {
        assert_eq!(format_size(2048), "2 KB");
    }

    #[test]
    fn format_size_mb() {
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn format_size_gb() {
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn android_entry_dir_size_display() {
        let e = AndroidEntry {
            name: "DCIM".into(),
            path: PathBuf::from("/sdcard/DCIM"),
            size: 4096,
            modified: "2024-01-10".into(),
            is_dir: true,
            is_symlink: false,
            is_hidden: false,
        };
        assert_eq!(e.size_display(), "--");
    }
}
