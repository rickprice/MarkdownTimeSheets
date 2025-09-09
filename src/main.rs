use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime};
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
struct TimeEntry {
    start_time: Option<NaiveTime>,
    end_time: Option<NaiveTime>,
    tentative: bool,
}

impl TimeEntry {
    fn new() -> Self {
        Self {
            start_time: None,
            end_time: None,
            tentative: false,
        }
    }

    fn duration(&self) -> Option<Duration> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => {
                if end >= start {
                    Some(end - start)
                } else {
                    Some(Duration::days(1) - (start - end))
                }
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct DaySummary {
    date: NaiveDate,
    total_duration: Duration,
    has_tentative: bool,
    has_incomplete: bool,
}

#[derive(Debug)]
struct WeekSummary {
    week_start: NaiveDate,
    total_duration: Duration,
    days: Vec<DaySummary>,
}

#[derive(Debug)]
struct MonthlySummary {
    year: i32,
    month: u32,
    total_duration: Duration,
}

struct TimesheetParser {
    start_regex: Regex,
    stop_regex: Regex,
    work_time_regex: Regex,
    holiday_regex: Regex,
    debug_mode: bool,
}

impl TimesheetParser {
    fn new(debug_mode: bool) -> Result<Self, regex::Error> {
        Ok(Self {
            start_regex: Regex::new(r"(?i)start(?:ed)?\s+work(?:ing)?(?:\s+at)?\s+(\d{1,2}):(\d{2})")?,
            stop_regex: Regex::new(r"(?i)stop(?:ped)?\s+work(?:ing)?(?:\s+at)?\s+(\d{1,2}):(\d{2})")?,
            work_time_regex: Regex::new(r"(?i)work\s+time\s+(\d+)\s+(minutes?|hours?)")?,
            holiday_regex: Regex::new(r"(?i)(stat(?:utory)?\s+holiday|pto|holiday\s+day)")?,
            debug_mode,
        })
    }

    fn apply_tentative_time(&self, entries: &mut [TimeEntry], date: NaiveDate) {
        let today = Local::now().date_naive();
        let is_today = date == today;

        if is_today && !entries.is_empty() {
            let last_entry = entries.last_mut().unwrap();
            if last_entry.start_time.is_some() && last_entry.end_time.is_none() {
                let current_time = Local::now().time();
                let start_time = last_entry.start_time.unwrap();
                
                if self.debug_mode {
                    eprintln!("DEBUG: Applying tentative time to last incomplete entry");
                    eprintln!("DEBUG: Start time: {start_time}, Current time: {current_time}");
                }
                
                let duration_from_start = if current_time >= start_time {
                    current_time - start_time
                } else {
                    Duration::days(1) - (start_time - current_time)
                };
                
                let tentative_end_time = if duration_from_start > Duration::hours(8) {
                    if self.debug_mode {
                        eprintln!("DEBUG: Duration {duration_from_start:?} exceeds 8 hours, capping at 8 hours");
                    }
                    start_time + Duration::hours(8)
                } else {
                    if self.debug_mode {
                        eprintln!("DEBUG: Using current time as end time (duration: {duration_from_start:?})");
                    }
                    current_time
                };
                
                last_entry.end_time = Some(tentative_end_time);
                last_entry.tentative = true;
                
                if self.debug_mode {
                    let final_duration = last_entry.duration();
                    eprintln!("DEBUG: Tentative end time: {tentative_end_time}, Final duration: {final_duration:?}");
                }
            }
        }
    }

    fn calculate_flags(entries: &[TimeEntry], has_orphaned_stop: bool, date: NaiveDate) -> (bool, bool) {
        let today = Local::now().date_naive();
        let is_today = date == today;
        
        let has_tentative = entries.iter().any(|entry| entry.tentative);
        
        let has_incomplete = if is_today {
            entries.iter().any(|entry| entry.start_time.is_some() && entry.end_time.is_none() && !entry.tentative) || has_orphaned_stop
        } else {
            entries.iter().any(|entry| entry.start_time.is_some() && entry.end_time.is_none()) || has_orphaned_stop
        };

        (has_tentative, has_incomplete)
    }

    #[allow(clippy::too_many_lines)]
    fn parse_file(&self, content: &str, date: NaiveDate) -> Result<DaySummary, Box<dyn std::error::Error>> {
        let mut entries = Vec::new();
        let mut current_entry = TimeEntry::new();
        let mut total_work_time_duration = Duration::zero();
        let today = Local::now().date_naive();
        let is_today = date == today;
        let mut has_orphaned_stop = false;

        if self.debug_mode {
            eprintln!("DEBUG: Parsing file for date: {date}");
            eprintln!("DEBUG: Is today: {is_today}");
            let line_count = content.lines().count();
            eprintln!("DEBUG: File content has {line_count} lines");
        }

        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1; // 1-indexed line numbers
            if let Some(caps) = self.start_regex.captures(line) {
                if current_entry.start_time.is_some() {
                    if self.debug_mode {
                        let start_time = current_entry.start_time;
                        eprintln!("DEBUG: Line {line_num}: Found overlapping start work entry (previous incomplete entry at {start_time:?})");
                    }
                    entries.push(current_entry);
                    current_entry = TimeEntry::new();
                }

                let hours: u32 = caps[1].parse()?;
                let minutes: u32 = caps[2].parse()?;
                
                if let Some(time) = NaiveTime::from_hms_opt(hours, minutes, 0) {
                    current_entry.start_time = Some(time);
                    if self.debug_mode {
                        let trimmed_line = line.trim();
                        eprintln!("DEBUG: Line {line_num}: Found start work at {time} (\"{trimmed_line}\")");
                    }
                } else if self.debug_mode {
                    eprintln!("DEBUG: Line {line_num}: Invalid time format {hours}:{minutes:02} in start work entry");
                }
            } else if let Some(caps) = self.stop_regex.captures(line) {
                let hours: u32 = caps[1].parse()?;
                let minutes: u32 = caps[2].parse()?;
                
                if let Some(time) = NaiveTime::from_hms_opt(hours, minutes, 0) {
                    if current_entry.start_time.is_some() {
                        // Normal case: stop time for existing start time
                        current_entry.end_time = Some(time);
                        if self.debug_mode {
                            let duration = current_entry.duration().unwrap_or(Duration::zero());
                            let trimmed_line = line.trim();
                            eprintln!("DEBUG: Line {line_num}: Found stop work at {time} (duration: {duration:?}) (\"{trimmed_line}\")");
                        }
                        entries.push(current_entry);
                        current_entry = TimeEntry::new();
                    } else {
                        // Error case: stop time without start time
                        has_orphaned_stop = true;
                        if self.debug_mode {
                            let trimmed_line = line.trim();
                            eprintln!("ERROR: Line {line_num}: Found stop work at {time} without corresponding start work (\"{trimmed_line}\")");
                        }
                    }
                } else if self.debug_mode {
                    eprintln!("DEBUG: Line {line_num}: Invalid time format {hours}:{minutes:02} in stop work entry");
                }
            } else if let Some(caps) = self.work_time_regex.captures(line) {
                let amount: u32 = caps[1].parse()?;
                let unit = caps[2].to_lowercase();
                
                let duration = if unit.starts_with("hour") {
                    Duration::hours(i64::from(amount))
                } else if unit.starts_with("minute") {
                    Duration::minutes(i64::from(amount))
                } else {
                    Duration::zero()
                };
                
                if self.debug_mode {
                    let trimmed_line = line.trim();
                    eprintln!("DEBUG: Line {line_num}: Found work time {amount} {unit} (duration: {duration:?}) (\"{trimmed_line}\")");
                }
                total_work_time_duration += duration;
            } else if self.holiday_regex.is_match(line) {
                if self.debug_mode {
                    let trimmed_line = line.trim();
                    eprintln!("DEBUG: Line {line_num}: Found holiday entry (8h 00m) (\"{trimmed_line}\")");
                }
                total_work_time_duration += Duration::hours(8);
            }
        }

        // Handle incomplete entry (start time but no stop time)
        if current_entry.start_time.is_some() {
            if self.debug_mode {
                let start_time = current_entry.start_time;
                eprintln!("DEBUG: End of file: Found incomplete entry with start time {start_time:?}");
            }
            entries.push(current_entry);
        }

        // Apply tentative time only to the last incomplete entry if it's today
        self.apply_tentative_time(&mut entries, date);

        let time_entries_duration: Duration = entries
            .iter()
            .filter_map(TimeEntry::duration)
            .sum();
        
        let total_duration = time_entries_duration + total_work_time_duration;
        let (has_tentative, has_incomplete) = Self::calculate_flags(&entries, has_orphaned_stop, date);

        if self.debug_mode {
            eprintln!("DEBUG: Parsing complete for {date}");
            let entries_len = entries.len();
            eprintln!("DEBUG: Found {entries_len} time entries");
            eprintln!("DEBUG: Time entries duration: {time_entries_duration:?}");
            eprintln!("DEBUG: Work time duration: {total_work_time_duration:?}");
            eprintln!("DEBUG: Total duration: {total_duration:?}");
            eprintln!("DEBUG: Has tentative: {has_tentative}");
            eprintln!("DEBUG: Has incomplete/errors: {has_incomplete}");
            if has_incomplete {
                let incomplete_entries = entries.iter().filter(|entry| entry.start_time.is_some() && entry.end_time.is_none() && !entry.tentative).count();
                if incomplete_entries > 0 {
                    eprintln!("ERROR: Found {incomplete_entries} incomplete time entries (start without stop)");
                }
                if has_orphaned_stop {
                    eprintln!("ERROR: Found orphaned stop entries (stop without start)");
                }
            }
            eprintln!("DEBUG: ----------------------------------------");
        }

        Ok(DaySummary {
            date,
            total_duration,
            has_tentative,
            has_incomplete,
        })
    }

    fn parse_directory(&self, dir_path: &Path) -> Result<Vec<DaySummary>, Box<dyn std::error::Error>> {
        let mut summaries = Vec::new();

        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "md") {
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(date) = NaiveDate::parse_from_str(filename, "%Y-%m-%d") {
                        let content = fs::read_to_string(&path)?;
                        let summary = self.parse_file(&content, date)?;
                        summaries.push(summary);
                    }
                }
            }
        }

        summaries.sort_unstable_by_key(|summary| summary.date);
        Ok(summaries)
    }

    fn group_by_week(summaries: &[DaySummary]) -> Vec<WeekSummary> {
        let mut weeks: HashMap<NaiveDate, Vec<DaySummary>> = HashMap::new();

        for summary in summaries {
            let week_start = summary.date - Duration::days(i64::from(summary.date.weekday().num_days_from_monday()));
            weeks.entry(week_start).or_default().push(summary.clone());
        }

        let mut week_summaries: Vec<_> = weeks
            .into_iter()
            .map(|(week_start, mut days)| {
                days.sort_unstable_by_key(|day| day.date);
                let total_duration = days
                    .iter()
                    .map(|day| day.total_duration)
                    .sum();

                WeekSummary {
                    week_start,
                    total_duration,
                    days,
                }
            })
            .collect();

        week_summaries.sort_unstable_by_key(|week| week.week_start);
        week_summaries
    }

    fn group_by_month(summaries: &[DaySummary]) -> Vec<MonthlySummary> {
        let mut months: HashMap<(i32, u32), Duration> = HashMap::new();

        for summary in summaries {
            let key = (summary.date.year(), summary.date.month());
            *months.entry(key).or_insert_with(Duration::zero) += summary.total_duration;
        }

        let mut monthly_summaries: Vec<_> = months
            .into_iter()
            .map(|((year, month), total_duration)| MonthlySummary {
                year,
                month,
                total_duration,
            })
            .collect();

        monthly_summaries.sort_unstable_by_key(|summary| (summary.year, summary.month));
        monthly_summaries
    }
}

fn format_duration(duration: Duration) -> String {
    let total_minutes = duration.num_minutes();
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{hours}h {minutes:02}m")
}

fn format_duration_with_flags(duration: Duration, has_tentative: bool, has_incomplete: bool) -> String {
    let total_minutes = duration.num_minutes();
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    
    let mut flags = String::new();
    if has_tentative {
        flags.push('*');
    }
    if has_incomplete {
        if !flags.is_empty() {
            flags.push(' ');
        }
        flags.push_str("E!");
    }
    
    if flags.is_empty() {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{hours}h {minutes:02}m {flags}")
    }
}

const MONTH_NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December"
];

fn get_month_name(month: u32) -> &'static str {
    MONTH_NAMES.get(month.saturating_sub(1) as usize).map_or("Unknown", |&name| name)
}

fn print_status_bar_summary(summaries: &[DaySummary], weeks: &[WeekSummary], weekly_hours: f64) {
    let today = chrono::Local::now().date_naive();
    
    // Find today's summary
    let today_summary = summaries.iter().find(|s| s.date == today);
    
    // Find current week's summary
    let current_week = weeks.iter().find(|week| {
        let week_end = week.week_start + Duration::days(6);
        today >= week.week_start && today <= week_end
    });
    
    match (today_summary, current_week) {
        (Some(day), Some(week)) => {
            let day_str = format_duration_with_flags(day.total_duration, day.has_tentative, day.has_incomplete);
            let week_str = format_duration(week.total_duration);
            
            #[allow(clippy::cast_precision_loss)]
            let week_hours = week.total_duration.num_minutes() as f64 / 60.0;
            let week_status = if week_hours < weekly_hours {
                let shortage = weekly_hours - week_hours;
                format!(" ({shortage:.1}h short)")
            } else {
                String::new()
            };
            
            println!("Today: {day_str} | Week: {week_str}{week_status}");
        }
        (Some(day), None) => {
            let day_str = format_duration_with_flags(day.total_duration, day.has_tentative, day.has_incomplete);
            println!("Today: {day_str} | Week: No data");
        }
        (None, Some(week)) => {
            let week_str = format_duration(week.total_duration);
            #[allow(clippy::cast_precision_loss)]
            let week_hours = week.total_duration.num_minutes() as f64 / 60.0;
            let week_status = if week_hours < weekly_hours {
                let shortage = weekly_hours - week_hours;
                format!(" ({shortage:.1}h short)")
            } else {
                String::new()
            };
            println!("Today: No data | Week: {week_str}{week_status}");
        }
        (None, None) => {
            println!("Today: No data | Week: No data");
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    let mut directory = ".";
    let mut weekly_hours = 40.0;
    let mut debug_mode = false;
    let mut summarize_mode = false;
    
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--weekly-hours" => {
                if let Some(value) = args.get(i + 1) {
                    weekly_hours = value.parse().unwrap_or(40.0);
                    i += 2;
                } else {
                    eprintln!("Error: --weekly-hours requires a value");
                    return Ok(());
                }
            }
            "--debug" => {
                debug_mode = true;
                i += 1;
            }
            "--summarize" => {
                summarize_mode = true;
                i += 1;
            }
            "--help" | "-h" => {
                println!("Usage: {} [directory] [--weekly-hours HOURS] [--debug] [--summarize]", args[0]);
                println!("  directory: Directory containing markdown timesheet files (default: current directory)");
                println!("  --weekly-hours: Expected weekly work hours (default: 40)");
                println!("  --debug: Show detailed debug information and error locations");
                println!("  --summarize: Show compact current day and week summary for status bar");
                return Ok(());
            }
            _ => {
                directory = &args[i];
                i += 1;
            }
        }
    }

    let parser = TimesheetParser::new(debug_mode)?;
    let summaries = parser.parse_directory(Path::new(directory))?;
    let weeks = TimesheetParser::group_by_week(&summaries);

    if summarize_mode {
        print_status_bar_summary(&summaries, &weeks, weekly_hours);
        return Ok(());
    }

    let months = TimesheetParser::group_by_month(&summaries);

    // Calculate the date two weeks ago from today
    let today = chrono::Local::now().date_naive();
    let two_weeks_ago = today - Duration::days(14);

    println!("Daily Summary (Last 2 Weeks):");
    println!("==============================");
    weeks
        .iter()
        .flat_map(|week| &week.days)
        .filter(|day| (day.total_duration > Duration::zero() || day.has_incomplete) && day.date >= two_weeks_ago)
        .for_each(|day| {
            let weekday = day.date.format("%a");
            println!("{} {:3} - {}", day.date, weekday, format_duration_with_flags(day.total_duration, day.has_tentative, day.has_incomplete));
        });

    println!("\nMonthly Summary:");
    println!("================");
    months
        .iter()
        .filter(|month| month.total_duration > Duration::zero())
        .for_each(|month| {
            println!("{} {}: {}", get_month_name(month.month), month.year, format_duration(month.total_duration));
        });

    println!("\nWeekly Summary:");
    println!("===============");
    weeks
        .iter()
        .filter(|week| week.total_duration > Duration::zero())
        .for_each(|week| {
            let week_end = week.week_start + Duration::days(6);
            #[allow(clippy::cast_precision_loss)]
            let actual_hours = week.total_duration.num_minutes() as f64 / 60.0;
            let formatted_duration = format_duration(week.total_duration);
            
            if actual_hours < weekly_hours {
                #[allow(clippy::cast_possible_truncation)]
                let difference_minutes = ((weekly_hours - actual_hours) * 60.0).round() as i64;
                let difference_duration = Duration::minutes(difference_minutes);
                println!(
                    "Week of {} - {}: {} [{}h {:02}m short]",
                    week.week_start,
                    week_end,
                    formatted_duration,
                    difference_duration.num_hours(),
                    difference_duration.num_minutes() % 60
                );
            } else {
                println!(
                    "Week of {} - {}: {}",
                    week.week_start,
                    week_end,
                    formatted_duration
                );
            }
        });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_time_entry_duration() {
        let mut entry = TimeEntry::new();
        entry.start_time = Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        entry.end_time = Some(NaiveTime::from_hms_opt(17, 30, 0).unwrap());

        let duration = entry.duration().unwrap();
        assert_eq!(duration.num_hours(), 8);
        assert_eq!(duration.num_minutes() % 60, 30);
    }

    #[test]
    fn test_time_entry_overnight() {
        let mut entry = TimeEntry::new();
        entry.start_time = Some(NaiveTime::from_hms_opt(23, 0, 0).unwrap());
        entry.end_time = Some(NaiveTime::from_hms_opt(1, 0, 0).unwrap());

        let duration = entry.duration().unwrap();
        assert_eq!(duration.num_hours(), 2);
    }

    #[test]
    fn test_time_entry_incomplete() {
        let mut entry = TimeEntry::new();
        entry.start_time = Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap());

        assert!(entry.duration().is_none());
    }

    #[test]
    fn test_parser_creation() {
        let parser = TimesheetParser::new(false);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_entry() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Start work 9:00\nSome notes\nStop work 17:30";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.date, date);
        assert_eq!(summary.total_duration.num_hours(), 8);
        assert_eq!(summary.total_duration.num_minutes() % 60, 30);
    }

    #[test]
    fn test_parse_multiple_entries() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = r#"
Start work 9:00
Stop work 12:00
Lunch break
Start work 13:00
Stop work 17:00
"#;
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 7);
    }

    #[test]
    fn test_parse_case_insensitive() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "START WORK 9:00\nstop Work 17:30";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_parse_incomplete_entry() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Start work 9:00\nSome work done but forgot to stop";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration, Duration::zero());
    }

    #[test]
    fn test_parse_invalid_times() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Start work 25:00\nStop work 12:70";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration, Duration::zero());
    }

    #[test]
    fn test_format_duration() {
        let duration = Duration::hours(8) + Duration::minutes(30);
        assert_eq!(format_duration(duration), "8h 30m");

        let duration = Duration::hours(0) + Duration::minutes(45);
        assert_eq!(format_duration(duration), "0h 45m");

        let duration = Duration::hours(10);
        assert_eq!(format_duration(duration), "10h 00m");
    }

    #[test]
    fn test_group_by_week() {
        let _parser = TimesheetParser::new(false).unwrap();
        let summaries = vec![
            DaySummary {
                date: NaiveDate::from_ymd_opt(2025, 8, 25).unwrap(), // Monday
                total_duration: Duration::hours(8),
                has_tentative: false,
                has_incomplete: false,
            },
            DaySummary {
                date: NaiveDate::from_ymd_opt(2025, 8, 26).unwrap(), // Tuesday
                total_duration: Duration::hours(7),
                has_tentative: false,
                has_incomplete: false,
            },
            DaySummary {
                date: NaiveDate::from_ymd_opt(2025, 9, 1).unwrap(), // Next Monday
                total_duration: Duration::hours(6),
                has_tentative: false,
                has_incomplete: false,
            },
        ];

        let weeks = TimesheetParser::group_by_week(&summaries);
        assert_eq!(weeks.len(), 2);
        
        assert_eq!(weeks[0].days.len(), 2);
        assert_eq!(weeks[0].total_duration.num_hours(), 15);
        
        assert_eq!(weeks[1].days.len(), 1);
        assert_eq!(weeks[1].total_duration.num_hours(), 6);
    }

    #[test]
    fn test_overlapping_entries() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Start work 9:00\nStart work 10:00\nStop work 17:00";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 7);
    }

    #[test]
    fn test_military_time_formats() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Start work 8:15\nStop work 16:45";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
        assert_eq!(summary.total_duration.num_minutes() % 60, 30);
    }

    #[test]
    fn test_single_digit_hours() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Start work 9:00\nStop work 5:00";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 20);
    }

    #[test]
    fn test_empty_file() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration, Duration::zero());
    }

    #[test]
    fn test_no_matching_lines() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "# Daily Notes\n\nWorked on project today.\nHad meetings.";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration, Duration::zero());
    }

    #[test]
    fn test_parse_different_formats() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = r#"
Started working at 8:30
Stopped working at 12:00
Start work 13:00
Stop work 17:30
Started work at 19:00
Stopped working 21:00
"#;
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 10);
        assert_eq!(summary.total_duration.num_minutes() % 60, 0);
    }

    #[test]
    fn test_work_time_minutes() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Work time 90 minutes read textbook";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 1);
        assert_eq!(summary.total_duration.num_minutes() % 60, 30);
    }

    #[test]
    fn test_work_time_hour() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Work time 1 hour did other work";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 1);
        assert_eq!(summary.total_duration.num_minutes() % 60, 0);
    }

    #[test]
    fn test_work_time_hours_plural() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Work time 3 hours completed project";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 3);
        assert_eq!(summary.total_duration.num_minutes() % 60, 0);
    }

    #[test]
    fn test_work_time_mixed_with_start_stop() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = r#"
Start work 9:00
Stop work 12:00
Work time 90 minutes read textbook
Work time 1 hour did other work
"#;
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 5);
        assert_eq!(summary.total_duration.num_minutes() % 60, 30);
    }

    #[test]
    fn test_work_time_case_insensitive() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "WORK TIME 45 MINUTES testing";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_minutes(), 45);
    }

    #[test]
    fn test_stat_holiday() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Stat holiday";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_statutory_holiday() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Statutory holiday";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_case_insensitive() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "STAT HOLIDAY";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_with_context() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Today was a stat holiday - Labour Day";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_mixed_with_other_entries() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = r#"
Start work 9:00
Stop work 12:00
Stat holiday
Work time 1 hour extra project
"#;
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 12);
    }

    #[test]
    fn test_multiple_holidays_same_day() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = r#"
Stat holiday
Statutory holiday mentioned again
"#;
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 16);
    }

    #[test]
    fn test_pto() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "PTO";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_pto_case_insensitive() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "pto";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_pto_with_context() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Taking PTO today for vacation";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_day() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Holiday day";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_day_case_insensitive() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "HOLIDAY DAY";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_day_with_context() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Christmas is a holiday day";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_mixed_holiday_types() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = r#"
Start work 9:00
Stop work 12:00
PTO
Holiday day
Stat holiday
"#;
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 27);
    }

    #[test]
    fn test_incomplete_entry_not_today() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = "Start work 9:00\nSome work done but forgot to stop";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap(); // Not today

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration, Duration::zero());
        assert!(!summary.has_tentative);
    }

    #[test] 
    fn test_format_duration_with_tentative() {
        let duration = Duration::hours(5) + Duration::minutes(30);
        assert_eq!(format_duration_with_flags(duration, false, false), "5h 30m");
        assert_eq!(format_duration_with_flags(duration, true, false), "5h 30m *");
    }

    #[test]
    fn test_multiple_incomplete_entries_only_last_gets_tentative() {
        let parser = TimesheetParser::new(false).unwrap();
        let content = r#"
Start work 8:00
Stop work 12:00
Start work 13:00
Start work 14:00
"#;
        let today = Local::now().date_naive();
        
        let summary = parser.parse_file(content, today).unwrap();
        // Should be: 4 hours (8-12) + tentative time from 14:00 to now (capped at 8 hours)
        // The 13:00 start should be ignored since it was overridden by 14:00 start
        assert!(summary.has_tentative);
        // The exact duration will depend on current time, but it should be > 4 hours
        assert!(summary.total_duration > Duration::hours(4));
    }

    #[test]
    fn test_current_time_used_as_stop_time_for_last_entry() {
        use chrono::Timelike;
        
        let parser = TimesheetParser::new(false).unwrap();
        // Use a start time very close to current time to avoid 8-hour cap issues
        let current_time = Local::now().time();
        let start_time = if current_time.hour() > 0 {
            NaiveTime::from_hms_opt(current_time.hour() - 1, current_time.minute(), 0).unwrap()
        } else {
            NaiveTime::from_hms_opt(23, current_time.minute(), 0).unwrap()
        };
        
        let content = format!("Start work {}:{:02}", start_time.hour(), start_time.minute());
        let today = Local::now().date_naive();
        
        let summary = parser.parse_file(&content, today).unwrap();
        assert!(summary.has_tentative);
        
        // Should have some duration (at least a few minutes, at most 8 hours)
        assert!(summary.total_duration > Duration::minutes(0));
        assert!(summary.total_duration <= Duration::hours(8));
    }

    #[test]
    fn test_incomplete_entry_flags() {
        let parser = TimesheetParser::new(false).unwrap();
        
        // Test incomplete entry on a non-today date
        let content = "Start work 9:00\nSome work done but forgot to stop";
        let past_date = NaiveDate::from_ymd_opt(2025, 8, 20).unwrap();
        
        let summary = parser.parse_file(content, past_date).unwrap();
        assert!(summary.has_incomplete);
        assert!(!summary.has_tentative);
        
        // Test complete entries
        let content = "Start work 9:00\nStop work 17:00";
        let summary = parser.parse_file(content, past_date).unwrap();
        assert!(!summary.has_incomplete);
        assert!(!summary.has_tentative);
        
        // Test today with incomplete entry (should get tentative, not incomplete)
        let content = "Start work 16:00";
        let today = Local::now().date_naive();
        let summary = parser.parse_file(content, today).unwrap();
        assert!(!summary.has_incomplete); // Today's incomplete entries become tentative
        assert!(summary.has_tentative);
    }

    #[test]
    fn test_orphaned_stop_entry_flags() {
        let parser = TimesheetParser::new(false).unwrap();
        
        // Test orphaned stop entry on a non-today date
        let content = "Some work done\nStop work 17:00";
        let past_date = NaiveDate::from_ymd_opt(2025, 8, 20).unwrap();
        
        let summary = parser.parse_file(content, past_date).unwrap();
        assert!(summary.has_incomplete); // Orphaned stop should flag as incomplete
        assert!(!summary.has_tentative);
        assert_eq!(summary.total_duration, Duration::zero()); // No duration from orphaned stop
        
        // Test orphaned stop on today's date
        let content = "Some work done\nStop work 17:00";
        let today = Local::now().date_naive();
        let summary = parser.parse_file(content, today).unwrap();
        assert!(summary.has_incomplete); // Orphaned stop should still flag as incomplete even for today
        assert!(!summary.has_tentative);
        
        // Test mixed: valid entry + orphaned stop
        let content = r#"
Start work 9:00
Stop work 12:00
Some notes
Stop work 17:00
"#;
        let summary = parser.parse_file(content, past_date).unwrap();
        assert!(summary.has_incomplete); // Should flag due to orphaned stop
        assert!(!summary.has_tentative);
        assert_eq!(summary.total_duration, Duration::hours(3)); // Only the valid 9-12 entry counts
    }

    #[test]
    fn test_format_duration_with_flags() {
        let duration = Duration::hours(5) + Duration::minutes(30);
        
        // No flags
        assert_eq!(format_duration_with_flags(duration, false, false), "5h 30m");
        
        // Tentative only
        assert_eq!(format_duration_with_flags(duration, true, false), "5h 30m *");
        
        // Incomplete only
        assert_eq!(format_duration_with_flags(duration, false, true), "5h 30m E!");
        
        // Both flags
        assert_eq!(format_duration_with_flags(duration, true, true), "5h 30m * E!");
    }
}