//! Calendar module (Phase 6.5): a month-grid calendar with per-day tasks.
//!
//! This module is pure in-memory state + navigation logic; it holds **no**
//! `App`, DB handle or networking. Tasks for the currently selected day are
//! loaded by the integrator from the DB and pushed in via [`CalendarPanel::set_tasks`].
//! Grid navigation never panics: all date arithmetic falls back to the current
//! selection when it would produce an invalid `NaiveDate`.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use ratatui::widgets::ListState;

/// One task pinned to a calendar day. Persisted by the integrator in `app.db`.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: i64,
    pub date: String, // "YYYY-MM-DD"
    pub text: String,
    pub done: bool,
}

/// Which sub-pane currently has focus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalFocus {
    Grid,
    Tasks,
}

pub struct CalendarPanel {
    /// Currently selected day.
    pub selected: NaiveDate,
    /// Cached "today" for the grid highlight.
    pub today: NaiveDate,
    /// Tasks for `selected` (loaded by App from DB).
    pub tasks: Vec<Task>,
    pub task_state: ListState,
    pub focus: CalFocus,
    /// Dates (in the visible month) that have at least one task — for grid dots.
    pub task_days: std::collections::HashSet<NaiveDate>,
    /// All tasks for the visible month, keyed by day, so each grid cell can
    /// render its own events inline.
    pub month_tasks: std::collections::HashMap<NaiveDate, Vec<Task>>,
    /// True while the inline task editor (in the Tasks pane) is active.
    pub editing: bool,
    /// Live text of the inline task editor.
    pub input: String,
}

impl Default for CalendarPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl CalendarPanel {
    pub fn new() -> Self {
        let today = chrono::Local::now().date_naive();
        let mut task_state = ListState::default();
        task_state.select(Some(0));
        Self {
            selected: today,
            today,
            tasks: Vec::new(),
            task_state,
            focus: CalFocus::Grid,
            task_days: std::collections::HashSet::new(),
            month_tasks: std::collections::HashMap::new(),
            editing: false,
            input: String::new(),
        }
    }

    /// First and last day of the currently visible month (month of `selected`).
    pub fn visible_month_range(&self) -> (NaiveDate, NaiveDate) {
        let (y, m) = (self.selected.year(), self.selected.month());
        let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(self.selected);
        let last = NaiveDate::from_ymd_opt(y, m, days_in_month(y, m)).unwrap_or(self.selected);
        (first, last)
    }

    /// Load a whole month's tasks (keyed by day). Refreshes the task-day markers
    /// and the selected day's task slice.
    pub fn load(&mut self, month_tasks: std::collections::HashMap<NaiveDate, Vec<Task>>) {
        self.task_days = month_tasks.keys().copied().collect();
        let day_tasks = month_tasks.get(&self.selected).cloned().unwrap_or_default();
        self.month_tasks = month_tasks;
        self.set_tasks(day_tasks);
    }

    /// Tasks for the given day in the loaded month (empty slice if none).
    pub fn tasks_on(&self, date: &NaiveDate) -> &[Task] {
        self.month_tasks
            .get(date)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Move the selection back one month, clamping the day to the last valid
    /// day of the target month (e.g. Jan 31 → Feb 28/29).
    pub fn prev_month(&mut self) {
        let (mut year, mut month) = (self.selected.year(), self.selected.month());
        if month == 1 {
            month = 12;
            year -= 1;
        } else {
            month -= 1;
        }
        self.set_year_month_clamped(year, month);
    }

    /// Move the selection forward one month, clamping the day as in [`prev_month`].
    pub fn next_month(&mut self) {
        let (mut year, mut month) = (self.selected.year(), self.selected.month());
        if month == 12 {
            month = 1;
            year += 1;
        } else {
            month += 1;
        }
        self.set_year_month_clamped(year, month);
    }

    fn set_year_month_clamped(&mut self, year: i32, month: u32) {
        let day = self.selected.day().min(days_in_month(year, month));
        if let Some(d) = NaiveDate::from_ymd_opt(year, month, day) {
            self.selected = d;
        }
    }

    /// Move the selection by `days` (may be negative); clamped to the valid
    /// `NaiveDate` range — falls back to the current selection on overflow.
    pub fn move_day(&mut self, days: i64) {
        if let Some(d) = self.selected.checked_add_signed(Duration::days(days)) {
            self.selected = d;
        }
    }

    /// Move the selection by whole weeks (may be negative).
    pub fn move_week(&mut self, weeks: i64) {
        self.move_day(weeks * 7);
    }

    pub fn goto_today(&mut self) {
        self.selected = self.today;
    }

    pub fn selected_date_str(&self) -> String {
        self.selected.format("%Y-%m-%d").to_string()
    }

    /// Replace the task list for the selected day, fixing the selection bounds.
    pub fn set_tasks(&mut self, tasks: Vec<Task>) {
        self.tasks = tasks;
        let sel = if self.tasks.is_empty() {
            None
        } else {
            Some(
                self.task_state
                    .selected()
                    .unwrap_or(0)
                    .min(self.tasks.len() - 1),
            )
        };
        self.task_state.select(sel);
    }

    pub fn selected_task(&self) -> Option<&Task> {
        self.task_state.selected().and_then(|i| self.tasks.get(i))
    }

    pub fn task_up(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = self.task_state.selected().unwrap_or(0);
        self.task_state.select(Some(i.saturating_sub(1)));
    }

    pub fn task_down(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = self.task_state.selected().unwrap_or(0);
        self.task_state
            .select(Some((i + 1).min(self.tasks.len() - 1)));
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            CalFocus::Grid => CalFocus::Tasks,
            CalFocus::Tasks => CalFocus::Grid,
        };
    }

    /// The visible month is the month of `selected`. Returns weeks Mon..Sun,
    /// each a `[Option<NaiveDate>; 7]` (None for days outside the month).
    pub fn weeks(&self) -> Vec<[Option<NaiveDate>; 7]> {
        let year = self.selected.year();
        let month = self.selected.month();
        let first = match NaiveDate::from_ymd_opt(year, month, 1) {
            Some(d) => d,
            None => return Vec::new(),
        };
        let n_days = days_in_month(year, month);
        // Leading blanks before the 1st (Monday = 0).
        let lead = first.weekday().num_days_from_monday() as usize;

        let mut weeks: Vec<[Option<NaiveDate>; 7]> = Vec::new();
        let mut row: [Option<NaiveDate>; 7] = [None; 7];
        let mut col = lead;

        for day in 1..=n_days {
            if let Some(d) = NaiveDate::from_ymd_opt(year, month, day) {
                row[col] = Some(d);
            }
            col += 1;
            if col == 7 {
                weeks.push(row);
                row = [None; 7];
                col = 0;
            }
        }
        if col != 0 {
            weeks.push(row);
        }
        weeks
    }
}

// ── Pure helpers (testable) ────────────────────────────────────────────────

/// English month name for `month` (1..=12); "?" for anything else.
pub fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "?",
    }
}

/// Number of days in `month` of `year`, accounting for leap February.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_name_bounds() {
        assert_eq!(month_name(1), "January");
        assert_eq!(month_name(12), "December");
        assert_eq!(month_name(0), "?");
        assert_eq!(month_name(13), "?");
    }

    #[test]
    fn days_in_month_handles_leap_and_30() {
        assert_eq!(days_in_month(2024, 2), 29); // leap
        assert_eq!(days_in_month(2023, 2), 28); // non-leap
        assert_eq!(days_in_month(2000, 2), 29); // divisible by 400
        assert_eq!(days_in_month(1900, 2), 28); // divisible by 100, not 400
        assert_eq!(days_in_month(2024, 4), 30); // April
        assert_eq!(days_in_month(2024, 1), 31); // January
    }

    #[test]
    fn weeks_cover_all_days_in_rows_of_seven() {
        // February 2024: leap month, 29 days, 1st is a Thursday.
        let mut cal = CalendarPanel::new();
        cal.selected = NaiveDate::from_ymd_opt(2024, 2, 15).unwrap();
        let weeks = cal.weeks();
        for row in &weeks {
            assert_eq!(row.len(), 7);
        }
        // Every day 1..=29 must appear exactly once.
        let mut seen = std::collections::HashSet::new();
        for row in &weeks {
            for cell in row.iter().flatten() {
                assert_eq!(cell.month(), 2);
                seen.insert(cell.day());
            }
        }
        assert_eq!(seen.len(), 29);
        for d in 1..=29u32 {
            assert!(seen.contains(&d), "missing day {d}");
        }
        // First row: Feb 1 2024 is a Thursday → 3 leading blanks (Mon,Tue,Wed).
        assert!(weeks[0][0].is_none());
        assert!(weeks[0][1].is_none());
        assert!(weeks[0][2].is_none());
        assert_eq!(weeks[0][3].unwrap().day(), 1);
    }

    #[test]
    fn month_navigation_clamps_day() {
        // Jan 31 → prev/next month must clamp to a valid day.
        let mut cal = CalendarPanel::new();
        cal.selected = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        cal.next_month(); // Feb 2024 has 29 days
        assert_eq!(cal.selected, NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());

        cal.selected = NaiveDate::from_ymd_opt(2023, 3, 31).unwrap();
        cal.prev_month(); // Feb 2023 has 28 days
        assert_eq!(cal.selected, NaiveDate::from_ymd_opt(2023, 2, 28).unwrap());

        // Year wrap.
        cal.selected = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        cal.prev_month();
        assert_eq!(cal.selected, NaiveDate::from_ymd_opt(2023, 12, 15).unwrap());
        cal.selected = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        cal.next_month();
        assert_eq!(cal.selected, NaiveDate::from_ymd_opt(2025, 1, 10).unwrap());
    }

    #[test]
    fn move_day_and_week() {
        let mut cal = CalendarPanel::new();
        cal.selected = NaiveDate::from_ymd_opt(2024, 6, 18).unwrap();
        cal.move_day(1);
        assert_eq!(cal.selected, NaiveDate::from_ymd_opt(2024, 6, 19).unwrap());
        cal.move_week(-1);
        assert_eq!(cal.selected, NaiveDate::from_ymd_opt(2024, 6, 12).unwrap());
    }

    #[test]
    fn set_tasks_fixes_selection_bounds() {
        let mut cal = CalendarPanel::new();
        cal.task_state.select(Some(5));
        cal.set_tasks(vec![]);
        assert_eq!(cal.task_state.selected(), None);
        cal.task_state.select(Some(5));
        cal.set_tasks(vec![Task {
            id: 1,
            date: "2024-06-18".into(),
            text: "x".into(),
            done: false,
        }]);
        assert_eq!(cal.task_state.selected(), Some(0));
    }
}
