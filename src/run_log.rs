use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

const RUN_LOG_DIR_NAME: &str = "verdarium-logs";
const RUN_LOG_FILE_PREFIX: &str = "verdarium-";
const RUN_LOG_FILE_SUFFIX: &str = ".log";
const RUN_LOG_LATEST_POINTER_FILE_NAME: &str = "latest-run-log.txt";
const MAX_RUN_LOG_FILES: usize = 10;

pub(crate) struct TeeLogWriter {
    file: File,
    stderr: io::Stderr,
}

impl TeeLogWriter {
    pub(crate) fn new(file: File) -> Self {
        Self {
            file,
            stderr: io::stderr(),
        }
    }
}

impl Write for TeeLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let stderr_result = self.stderr.write_all(buf);
        let file_result = self.file.write_all(buf);
        stderr_result?;
        file_result?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let stderr_result = self.stderr.flush();
        let file_result = self.file.flush();
        stderr_result?;
        file_result
    }
}

pub(crate) fn run_log_dir() -> PathBuf {
    verdarium_vkn::project_root()
        .join("target")
        .join(RUN_LOG_DIR_NAME)
}

fn latest_run_log_pointer_path(dir: &Path) -> PathBuf {
    dir.join(RUN_LOG_LATEST_POINTER_FILE_NAME)
}

fn is_run_log_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    file_name.starts_with(RUN_LOG_FILE_PREFIX) && file_name.ends_with(RUN_LOG_FILE_SUFFIX)
}

fn write_latest_run_log_pointer(dir: &Path, log_path: &Path) -> io::Result<()> {
    fs::write(
        latest_run_log_pointer_path(dir),
        format!("{}\n", log_path.display()),
    )
}

fn scan_latest_run_log_path(dir: &Path) -> io::Result<Option<PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }

    let mut logs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !is_run_log_file(&path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            logs.push(path);
        }
    }

    logs.sort();
    Ok(logs.pop())
}

pub(crate) fn latest_run_log_path() -> io::Result<Option<PathBuf>> {
    latest_run_log_path_in_dir(&run_log_dir())
}

fn latest_run_log_path_in_dir(dir: &Path) -> io::Result<Option<PathBuf>> {
    let pointer_path = latest_run_log_pointer_path(dir);
    if let Ok(contents) = fs::read_to_string(&pointer_path) {
        let path = PathBuf::from(contents.trim());
        if is_run_log_file(&path) && path.is_file() {
            return Ok(Some(path));
        }
    }

    scan_latest_run_log_path(dir)
}

pub(crate) fn tail_log_file(path: &Path, line_count: usize) -> io::Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    tail_log_file_to_writer(path, line_count, &mut stdout)
}

fn tail_log_file_to_writer<W: Write>(
    path: &Path,
    line_count: usize,
    writer: &mut W,
) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let lines = content.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(line_count);
    for line in &lines[start..] {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

fn prune_old_run_logs(dir: &Path, current_path: &Path) -> io::Result<()> {
    let mut logs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !is_run_log_file(&path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        logs.push(path);
    }

    if logs.len() <= MAX_RUN_LOG_FILES {
        return Ok(());
    }

    logs.sort();

    let remove_count = logs.len() - MAX_RUN_LOG_FILES;
    for path in logs
        .into_iter()
        .filter(|path| path != current_path)
        .take(remove_count)
    {
        if let Err(err) = fs::remove_file(&path) {
            eprintln!("Failed to remove old run log {}: {}", path.display(), err);
        }
    }

    Ok(())
}

pub(crate) fn create_run_log_file() -> io::Result<(PathBuf, File)> {
    let dir = run_log_dir();
    create_run_log_file_in_dir(&dir)
}

fn create_run_log_file_in_dir(dir: &Path) -> io::Result<(PathBuf, File)> {
    fs::create_dir_all(dir)?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
    let process_id = std::process::id();
    for attempt in 0..100 {
        let attempt_suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let path = dir.join(format!(
            "{RUN_LOG_FILE_PREFIX}{timestamp}-{process_id}{attempt_suffix}{RUN_LOG_FILE_SUFFIX}"
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                if let Err(err) = write_latest_run_log_pointer(dir, &path) {
                    eprintln!(
                        "Failed to update latest run log pointer {}: {}",
                        latest_run_log_pointer_path(dir).display(),
                        err
                    );
                }
                if let Err(err) = prune_old_run_logs(dir, &path) {
                    eprintln!("Failed to prune old run logs in {}: {}", dir.display(), err);
                }
                return Ok((path, file));
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "failed to reserve a unique run log path in {}",
            dir.display()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_log_dir(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "verdarium-run-log-test-{test_name}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn run_log_file_filter_matches_only_expected_names() {
        assert!(is_run_log_file(Path::new(
            "verdarium-20260601-010203.456-1.log"
        )));
        assert!(!is_run_log_file(Path::new("latest-run-log.txt")));
        assert!(!is_run_log_file(Path::new(
            "verdarium-20260601-010203.456-1.txt"
        )));
        assert!(!is_run_log_file(Path::new("other-verdarium-20260601.log")));
    }

    #[test]
    fn latest_path_prefers_valid_pointer_then_falls_back_to_scan() {
        let dir = temp_log_dir("latest");
        fs::create_dir_all(&dir).unwrap();
        let old_log = dir.join("verdarium-20260601-010000.000-1.log");
        let new_log = dir.join("verdarium-20260601-020000.000-1.log");
        let pointed_log = dir.join("verdarium-20260601-030000.000-1.log");
        fs::write(&old_log, "old").unwrap();
        fs::write(&new_log, "new").unwrap();
        fs::write(&pointed_log, "pointed").unwrap();
        fs::write(
            latest_run_log_pointer_path(&dir),
            format!("{}\n", pointed_log.display()),
        )
        .unwrap();

        assert_eq!(
            latest_run_log_path_in_dir(&dir).unwrap(),
            Some(pointed_log.clone())
        );

        fs::write(
            latest_run_log_pointer_path(&dir),
            "/missing/verdarium-missing.log\n",
        )
        .unwrap();
        assert_eq!(latest_run_log_path_in_dir(&dir).unwrap(), Some(pointed_log));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tail_log_file_writes_requested_suffix() {
        let dir = temp_log_dir("tail");
        fs::create_dir_all(&dir).unwrap();
        let log = dir.join("verdarium-20260601-010000.000-1.log");
        fs::write(&log, "a\nb\nc\nd\n").unwrap();

        let mut output = Vec::new();
        tail_log_file_to_writer(&log, 2, &mut output).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "c\nd\n");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn create_run_log_file_updates_pointer_and_prunes_old_logs() {
        let dir = temp_log_dir("create");
        fs::create_dir_all(&dir).unwrap();
        for idx in 0..MAX_RUN_LOG_FILES {
            let path = dir.join(format!("verdarium-20260601-0100{idx:02}.000-1.log"));
            fs::write(path, "old").unwrap();
        }

        let (created_path, mut file) = create_run_log_file_in_dir(&dir).unwrap();
        writeln!(file, "created").unwrap();
        drop(file);

        let pointed = fs::read_to_string(latest_run_log_pointer_path(&dir)).unwrap();
        assert_eq!(PathBuf::from(pointed.trim()), created_path);
        let remaining_logs = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| is_run_log_file(&entry.path()))
            .count();
        assert_eq!(remaining_logs, MAX_RUN_LOG_FILES);

        fs::remove_dir_all(&dir).unwrap();
    }
}
