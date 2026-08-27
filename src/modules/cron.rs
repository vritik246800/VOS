use anyhow::Result;
use std::io::Write;
use std::process::Command;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CronLine {
    Job(CronJob),
    Comment(String),
    Empty,
}

#[derive(Debug, Clone)]
pub struct CronJob {
    pub minute: String,
    pub hour: String,
    pub day: String,
    pub month: String,
    pub weekday: String,
    pub command: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Editor state
// ---------------------------------------------------------------------------

pub struct CronEditor {
    pub lines: Vec<CronLine>,
    pub selected: usize,
    pub editing: Option<usize>,
    /// [minute, hour, day, month, weekday, command]
    pub edit_fields: [String; 6],
    pub edit_cursor: usize,
    pub status_msg: String,
    pub dirty: bool,
}

// ---------------------------------------------------------------------------
// impl CronEditor
// ---------------------------------------------------------------------------

impl CronEditor {
    pub fn new() -> Self {
        let lines = Self::load();
        Self {
            lines,
            selected: 0,
            editing: None,
            edit_fields: Default::default(),
            edit_cursor: 0,
            status_msg: String::new(),
            dirty: false,
        }
    }

    // -----------------------------------------------------------------------
    // Load / parse
    // -----------------------------------------------------------------------

    pub fn load() -> Vec<CronLine> {
        let output = Command::new("crontab").arg("-l").output();

        let text = match output {
            Err(_) => return Vec::new(),
            Ok(out) => {
                // exit code 1 with empty stdout means "no crontab for user"
                if !out.status.success() && out.stdout.is_empty() {
                    return Vec::new();
                }
                String::from_utf8_lossy(&out.stdout).into_owned()
            }
        };

        text.lines().map(Self::parse_line).collect()
    }

    fn parse_line(line: &str) -> CronLine {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            return CronLine::Empty;
        }

        if trimmed.starts_with('#') {
            return CronLine::Comment(line.to_owned());
        }

        // Handle @-shortcuts
        if let Some(rest) = trimmed.strip_prefix('@') {
            let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
            if parts.len() == 2 {
                let keyword = parts[0];
                let cmd = parts[1].trim().to_owned();
                let (min, hour, day, month, weekday) = match keyword {
                    "reboot" => (
                        "@reboot".to_owned(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ),
                    "hourly" => (
                        "0".to_owned(),
                        "*".to_owned(),
                        "*".to_owned(),
                        "*".to_owned(),
                        "*".to_owned(),
                    ),
                    "daily" | "midnight" => (
                        "0".to_owned(),
                        "0".to_owned(),
                        "*".to_owned(),
                        "*".to_owned(),
                        "*".to_owned(),
                    ),
                    "weekly" => (
                        "0".to_owned(),
                        "0".to_owned(),
                        "*".to_owned(),
                        "*".to_owned(),
                        "0".to_owned(),
                    ),
                    "monthly" => (
                        "0".to_owned(),
                        "0".to_owned(),
                        "1".to_owned(),
                        "*".to_owned(),
                        "*".to_owned(),
                    ),
                    "yearly" | "annually" => (
                        "0".to_owned(),
                        "0".to_owned(),
                        "1".to_owned(),
                        "1".to_owned(),
                        "*".to_owned(),
                    ),
                    _ => return CronLine::Comment(line.to_owned()),
                };

                let description = if keyword == "reboot" {
                    "at system reboot".to_owned()
                } else {
                    Self::describe_schedule_static(&min, &hour, &day, &month, &weekday)
                };

                return CronLine::Job(CronJob {
                    minute: min,
                    hour,
                    day,
                    month,
                    weekday,
                    command: cmd,
                    description,
                });
            }
        }

        // Standard 5-field cron line
        let fields: Vec<&str> = trimmed.splitn(6, char::is_whitespace).collect();
        if fields.len() >= 6 {
            let min = fields[0].to_owned();
            let hour = fields[1].to_owned();
            let day = fields[2].to_owned();
            let mon = fields[3].to_owned();
            let wday = fields[4].to_owned();
            let cmd = fields[5].trim().to_owned();
            let desc = Self::describe_schedule_static(&min, &hour, &day, &mon, &wday);
            return CronLine::Job(CronJob {
                minute: min,
                hour,
                day,
                month: mon,
                weekday: wday,
                command: cmd,
                description: desc,
            });
        }

        // Fallback: treat as a comment to preserve the line
        CronLine::Comment(line.to_owned())
    }

    // -----------------------------------------------------------------------
    // Schedule description
    // -----------------------------------------------------------------------

    fn describe_schedule_static(
        min: &str,
        hour: &str,
        day: &str,
        month: &str,
        weekday: &str,
    ) -> String {
        // Special: @reboot sentinel
        if min == "@reboot" {
            return "at system reboot".to_owned();
        }

        match (min, hour, day, month, weekday) {
            ("*", "*", "*", "*", "*") => "every minute".to_owned(),
            ("0", "*", "*", "*", "*") => "every hour".to_owned(),
            ("0", "0", "*", "*", "*") => "every day at midnight".to_owned(),
            ("0", h, "*", "*", "*") => format!("every day at {:0>2}:00", h),
            ("0", "0", "*", "*", d) if !d.contains('*') => {
                let day_name = weekday_name(d);
                format!("every {} at midnight", day_name)
            }
            ("0", h, "*", "*", "1-5") => format!("weekdays at {:0>2}:00", h),
            ("0", h, "*", "*", "6-7") | ("0", h, "*", "*", "0,6") => {
                format!("weekends at {:0>2}:00", h)
            }
            ("0", "0", "1", "*", "*") => "first of every month at midnight".to_owned(),
            ("0", "0", "1", "1", "*") => "yearly on Jan 1 at midnight".to_owned(),
            _ => format!("{} {} {} {} {}", min, hour, day, month, weekday),
        }
    }

    pub fn describe_schedule(
        min: &str,
        hour: &str,
        day: &str,
        month: &str,
        weekday: &str,
    ) -> String {
        Self::describe_schedule_static(min, hour, day, month, weekday)
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

    pub fn move_up(&mut self) {
        if self.selected == 0 || self.lines.is_empty() {
            return;
        }
        let mut i = self.selected - 1;
        loop {
            if matches!(&self.lines[i], CronLine::Job(_)) {
                self.selected = i;
                return;
            }
            if i == 0 {
                return;
            }
            i -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        let mut i = self.selected + 1;
        while i < self.lines.len() {
            if matches!(&self.lines[i], CronLine::Job(_)) {
                self.selected = i;
                return;
            }
            i += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Editing
    // -----------------------------------------------------------------------

    pub fn begin_edit(&mut self, idx: usize) {
        if let Some(CronLine::Job(job)) = self.lines.get(idx) {
            self.edit_fields = [
                job.minute.clone(),
                job.hour.clone(),
                job.day.clone(),
                job.month.clone(),
                job.weekday.clone(),
                job.command.clone(),
            ];
            self.edit_cursor = 0;
            self.editing = Some(idx);
        }
    }

    pub fn save_edit(&mut self) {
        if let Some(idx) = self.editing {
            let [ref min, ref hour, ref day, ref month, ref weekday, ref cmd] =
                self.edit_fields.clone();
            let description = Self::describe_schedule_static(min, hour, day, month, weekday);
            if let Some(line) = self.lines.get_mut(idx) {
                *line = CronLine::Job(CronJob {
                    minute: min.clone(),
                    hour: hour.clone(),
                    day: day.clone(),
                    month: month.clone(),
                    weekday: weekday.clone(),
                    command: cmd.clone(),
                    description,
                });
            }
            self.editing = None;
            self.dirty = true;
            self.status_msg = "Job updated (not saved — press [w] to write crontab)".to_owned();
        }
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
        self.status_msg = "Edit cancelled.".to_owned();
    }

    // -----------------------------------------------------------------------
    // CRUD helpers
    // -----------------------------------------------------------------------

    pub fn delete_selected(&mut self) {
        if self.selected < self.lines.len() {
            self.lines.remove(self.selected);
            if self.selected > 0 && self.selected >= self.lines.len() {
                self.selected = self.lines.len().saturating_sub(1);
            }
            self.dirty = true;
            self.status_msg = "Line deleted.".to_owned();
        }
    }

    pub fn add_new_job(&mut self) {
        let new_job = CronLine::Job(CronJob {
            minute: "*".to_owned(),
            hour: "*".to_owned(),
            day: "*".to_owned(),
            month: "*".to_owned(),
            weekday: "*".to_owned(),
            command: String::new(),
            description: "every minute".to_owned(),
        });
        self.lines.push(new_job);
        let idx = self.lines.len() - 1;
        self.selected = idx;
        self.begin_edit(idx);
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    pub fn write_crontab(&self) -> Result<()> {
        // Build crontab text
        let text = self
            .lines
            .iter()
            .map(|l| match l {
                CronLine::Job(job) => {
                    if job.minute == "@reboot" {
                        format!("@reboot {}", job.command)
                    } else {
                        format!(
                            "{} {} {} {} {} {}",
                            job.minute, job.hour, job.day, job.month, job.weekday, job.command
                        )
                    }
                }
                CronLine::Comment(s) => s.clone(),
                CronLine::Empty => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Backup
        std::fs::create_dir_all("data")?;
        std::fs::write("data/crontab.bak", &text)?;

        // Write to temp file and install
        let tmp = tempfile_path();
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)?;
            f.write_all(text.as_bytes())?;
            // crontab requires a trailing newline
            if !text.ends_with('\n') {
                f.write_all(b"\n")?;
            }
        }

        let status = Command::new("crontab").arg(&tmp).status()?;
        let _ = std::fs::remove_file(&tmp);

        if !status.success() {
            anyhow::bail!("crontab command failed with status {}", status);
        }
        Ok(())
    }
}

impl Default for CronEditor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn weekday_name(d: &str) -> &str {
    match d {
        "0" | "7" => "Sunday",
        "1" => "Monday",
        "2" => "Tuesday",
        "3" => "Wednesday",
        "4" => "Thursday",
        "5" => "Friday",
        "6" => "Saturday",
        _ => d,
    }
}

fn tempfile_path() -> std::path::PathBuf {
    let pid = std::process::id();
    std::env::temp_dir().join(format!("vos_cron_{}.tmp", pid))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
# System crontab
# Edited by VOS

@daily /usr/bin/logrotate /etc/logrotate.conf
@reboot /usr/sbin/ntpdate pool.ntp.org

0 9 * * 1-5 /home/user/scripts/standup.sh

*/15 * * * * /home/user/check.sh

# End of file
";

    fn parse_fixture() -> Vec<CronLine> {
        FIXTURE.lines().map(CronEditor::parse_line).collect()
    }

    #[test]
    fn test_comment_lines() {
        let lines = parse_fixture();
        assert!(matches!(&lines[0], CronLine::Comment(s) if s.starts_with("# System")));
        assert!(matches!(&lines[1], CronLine::Comment(s) if s.starts_with("# Edited")));
    }

    #[test]
    fn test_empty_line() {
        let lines = parse_fixture();
        assert!(matches!(&lines[2], CronLine::Empty));
    }

    #[test]
    fn test_at_daily() {
        let lines = parse_fixture();
        if let CronLine::Job(job) = &lines[3] {
            assert_eq!(job.minute, "0");
            assert_eq!(job.hour, "0");
            assert_eq!(job.command, "/usr/bin/logrotate /etc/logrotate.conf");
            assert_eq!(job.description, "every day at midnight");
        } else {
            panic!("Expected Job at index 3, got {:?}", lines[3]);
        }
    }

    #[test]
    fn test_at_reboot() {
        let lines = parse_fixture();
        if let CronLine::Job(job) = &lines[4] {
            assert_eq!(job.minute, "@reboot");
            assert_eq!(job.command, "/usr/sbin/ntpdate pool.ntp.org");
            assert_eq!(job.description, "at system reboot");
        } else {
            panic!("Expected Job at index 4, got {:?}", lines[4]);
        }
    }

    #[test]
    fn test_standard_job() {
        let lines = parse_fixture();
        // index 5 = blank, index 6 = standup job
        if let CronLine::Job(job) = &lines[6] {
            assert_eq!(job.minute, "0");
            assert_eq!(job.hour, "9");
            assert_eq!(job.weekday, "1-5");
            assert_eq!(job.command, "/home/user/scripts/standup.sh");
            assert_eq!(job.description, "weekdays at 09:00");
        } else {
            panic!("Expected Job at index 6, got {:?}", lines[6]);
        }
    }

    #[test]
    fn test_every_15_minutes() {
        let lines = parse_fixture();
        // index 7 = blank, index 8 = check.sh
        if let CronLine::Job(job) = &lines[8] {
            assert_eq!(job.minute, "*/15");
            assert_eq!(job.command, "/home/user/check.sh");
        } else {
            panic!("Expected Job at index 8, got {:?}", lines[8]);
        }
    }

    #[test]
    fn test_describe_every_minute() {
        assert_eq!(
            CronEditor::describe_schedule("*", "*", "*", "*", "*"),
            "every minute"
        );
    }

    #[test]
    fn test_describe_every_hour() {
        assert_eq!(
            CronEditor::describe_schedule("0", "*", "*", "*", "*"),
            "every hour"
        );
    }

    #[test]
    fn test_describe_midnight() {
        assert_eq!(
            CronEditor::describe_schedule("0", "0", "*", "*", "*"),
            "every day at midnight"
        );
    }

    #[test]
    fn test_describe_weekdays() {
        assert_eq!(
            CronEditor::describe_schedule("0", "9", "*", "*", "1-5"),
            "weekdays at 09:00"
        );
    }

    #[test]
    fn test_describe_complex_fallback() {
        let desc = CronEditor::describe_schedule("*/15", "*", "*", "*", "*");
        assert_eq!(desc, "*/15 * * * *");
    }
}
