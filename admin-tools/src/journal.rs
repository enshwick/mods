//! The mod's logs in the world's data folder, the audit log and the chat
//! log: one line per event, appended by a thread of its own, so the game
//! thread never waits on a write, and moved aside past a size.

use std::fs::{self, OpenOptions};
use std::io::{self, Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use enshwick::Relock as _;

/// The audit log: one line per admin action.
pub(crate) const AUDIT: Files = Files {
    now: "audit.log",
    old: "audit.1.log",
};

/// The chat log: one line per chat line the server heard.
pub(crate) const CHAT: Files = Files {
    now: "chat.log",
    old: "chat.1.log",
};

/// The size past which a log moves to its older file and starts again, so
/// the two files together hold at most about twice this.
pub(crate) const MAX_BYTES: u64 = 1 << 20;

/// The most bytes a read of the log's last lines takes from its end.
const TAIL_BYTES: u64 = 64 << 10;

/// A log's two files in the world's data folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Files {
    /// The file new lines go to.
    pub now: &'static str,
    /// The older half, once the log outgrew [`MAX_BYTES`].
    pub old: &'static str,
}

/// A line on its way to a world's log.
struct Entry {
    dir: PathBuf,
    line: String,
}

/// A log's writer: lines go to a thread that appends them.
#[derive(Debug)]
pub(crate) struct Journal {
    files: Files,
    writer: Mutex<Option<Writer>>,
}

/// The thread that appends lines and the way to hand it one.
#[derive(Debug)]
struct Writer {
    lines: Sender<Entry>,
    thread: Option<JoinHandle<()>>,
}

impl Journal {
    /// A writer of the log `files` whose thread starts with the first line.
    pub(crate) const fn new(files: Files) -> Self {
        Self {
            files,
            writer: Mutex::new(None),
        }
    }

    /// Appends `fields`, tab-separated after the time, to the log in `dir`;
    /// tabs and line breaks inside a field become spaces.
    pub(crate) fn record(&self, dir: PathBuf, fields: &[&str]) {
        let mut line = utc(now());
        for field in fields {
            line.push('\t');
            line.extend(field.chars().map(|c| if c.is_control() { ' ' } else { c }));
        }
        line.push('\n');
        let mut writer = self.writer.relock();
        let started = writer.get_or_insert_with(|| spawn(self.files));
        if let Err(lost) = started.lines.send(Entry { dir, line }) {
            // The thread ended: start another and hand it the line.
            let _sent = writer.insert(spawn(self.files)).lines.send(lost.0);
        }
    }

    /// Writes every line still on its way and ends the thread; the next
    /// line starts another.
    pub(crate) fn finish(&self) {
        let Some(Writer { lines, thread }) = self.writer.relock().take() else {
            return;
        };
        drop(lines);
        if thread.is_some_and(|thread| thread.join().is_err()) {
            enshwick::warn!("{}: the writer thread panicked", self.files.now);
        }
    }
}

/// Starts the writer thread of `files`; its lines arrive through the
/// sender.
fn spawn(files: Files) -> Writer {
    let (sender, lines) = mpsc::channel::<Entry>();
    let started = std::thread::Builder::new()
        .name(format!("admin-tools {}", files.now))
        .spawn(move || {
            for entry in lines {
                if let Err(why) = files.append(&entry.dir, &entry.line) {
                    enshwick::warn!("{} not written: {why}", entry.dir.join(files.now).display());
                }
            }
        });
    let thread = started
        .inspect_err(|why| enshwick::warn!("{}: no writer thread: {why}", files.now))
        .ok();
    Writer {
        lines: sender,
        thread,
    }
}

impl Files {
    /// Appends `line` to the log in `dir`, first moving a log past
    /// [`MAX_BYTES`] to the older file.
    pub(crate) fn append(self, dir: &Path, line: &str) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join(self.now);
        if fs::metadata(&path).is_ok_and(|meta| meta.len() >= MAX_BYTES) {
            fs::rename(&path, dir.join(self.old))?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?
            .write_all(line.as_bytes())
    }

    /// The last `count` lines of the log in `dir`, oldest first, taking the
    /// rest from the older file when the log moved aside lately; empty when
    /// there is no log.
    ///
    /// # Errors
    ///
    /// The reason a log that is there does not read.
    pub(crate) fn tail(self, dir: &Path, count: usize) -> io::Result<Vec<String>> {
        let mut lines = tail_of(&dir.join(self.now), count)?;
        if lines.len() < count {
            let mut older = tail_of(&dir.join(self.old), count - lines.len())?;
            older.append(&mut lines);
            lines = older;
        }
        Ok(lines)
    }
}

/// The last `count` lines of the file at `path`; empty when it is not
/// there.
fn tail_of(path: &Path, count: usize) -> io::Result<Vec<String>> {
    let Some(mut file) = fs::File::open(path).map(Some).or_else(|why| {
        if why.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(why)
        }
    })?
    else {
        return Ok(Vec::new());
    };
    let start = file.seek(SeekFrom::End(0))?.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    // A read that starts mid-file starts mid-line: drop that part line.
    let whole = if start > 0 {
        text.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &text
    };
    let lines: Vec<&str> = whole.lines().collect();
    Ok(lines[lines.len().saturating_sub(count)..]
        .iter()
        .map(|line| (*line).to_owned())
        .collect())
}

/// Seconds since 1970, UTC.
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// `seconds` since 1970 as `YYYY-MM-DD HH:MM:SS`, UTC, by the civil-from-days
/// algorithm of Howard Hinnant's date library.
pub(crate) fn utc(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Days since 0000-03-01, so a year's leap day comes last.
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn append(dir: &Path, line: &str) -> io::Result<()> {
        AUDIT.append(dir, line)
    }

    fn tail(dir: &Path, count: usize) -> io::Result<Vec<String>> {
        AUDIT.tail(dir, count)
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("admin-tools-journal-{name}-{}", std::process::id()));
        let _gone = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn utc_dates_match_known_days_across_leap_years_and_centuries() {
        assert_eq!(utc(0), "1970-01-01 00:00:00");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(utc(1_709_251_199), "2024-02-29 23:59:59");
        assert_eq!(utc(1_793_793_600), "2026-11-04 12:00:00");
        assert_eq!(utc(4_107_542_400), "2100-03-01 00:00:00");
    }

    #[test]
    fn the_log_moves_aside_past_its_size_and_starts_again() {
        let dir = scratch("rotate");
        fs::write(
            dir.join(AUDIT.now),
            vec![b'x'; usize::try_from(MAX_BYTES).unwrap()],
        )
        .unwrap();
        append(&dir, "next\n").unwrap();
        assert_eq!(fs::read_to_string(dir.join(AUDIT.now)).unwrap(), "next\n");
        assert_eq!(fs::metadata(dir.join(AUDIT.old)).unwrap().len(), MAX_BYTES);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_tail_gives_the_last_lines_and_nothing_without_a_log() {
        let dir = scratch("tail");
        assert!(tail(&dir, 5).unwrap().is_empty());
        for n in 0..10 {
            append(&dir, &format!("line {n}\n")).unwrap();
        }
        assert_eq!(tail(&dir, 3).unwrap(), ["line 7", "line 8", "line 9"]);
        assert_eq!(tail(&dir, 50).unwrap().len(), 10);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_tail_of_a_long_log_starts_at_a_whole_line() {
        let dir = scratch("long");
        let line = format!("{}\n", "y".repeat(99));
        for _ in 0..2000 {
            append(&dir, &line).unwrap();
        }
        let last = tail(&dir, 5000).unwrap();
        assert!(last.iter().all(|l| l.len() == 99));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_tail_right_after_a_move_aside_takes_the_rest_from_the_older_file() {
        let dir = scratch("tail-rotated");
        fs::write(dir.join(AUDIT.old), "old 1\nold 2\nold 3\n").unwrap();
        append(&dir, "new 1\n").unwrap();
        assert_eq!(tail(&dir, 3).unwrap(), ["old 2", "old 3", "new 1"]);
        assert_eq!(tail(&dir, 1).unwrap(), ["new 1"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finish_writes_every_line_still_on_its_way_before_it_returns() {
        let dir = scratch("finish");
        let audit = Journal::new(AUDIT);
        for n in 0..200 {
            audit.record(dir.clone(), &[&format!("line {n}")]);
        }
        audit.finish();
        assert_eq!(tail(&dir, 500).unwrap().len(), 200);
        audit.record(dir.clone(), &["after"]);
        audit.finish();
        assert!(tail(&dir, 1).unwrap()[0].ends_with("\tafter"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_record_reaches_the_file_with_control_characters_as_spaces() {
        let dir = scratch("record");
        let audit = Journal::new(AUDIT);
        audit.record(dir.clone(), &["Ann", "kick\tBob\nnow"]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while tail(&dir, 1).unwrap().is_empty() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        let line = tail(&dir, 1).unwrap().remove(0);
        assert!(line.ends_with("\tAnn\tkick Bob now"), "{line}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
