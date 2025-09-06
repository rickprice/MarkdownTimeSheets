use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
struct TimeEntry {
    start_time: Option<NaiveTime>,
    end_time: Option<NaiveTime>,
}

impl TimeEntry {
    fn new() -> Self {
        Self {
            start_time: None,
            end_time: None,
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
}

impl TimesheetParser {
    fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            start_regex: Regex::new(r"(?i)start(?:ed)?\s+work(?:ing)?(?:\s+at)?\s+(\d{1,2}):(\d{2})")?,
            stop_regex: Regex::new(r"(?i)stop(?:ped)?\s+work(?:ing)?(?:\s+at)?\s+(\d{1,2}):(\d{2})")?,
            work_time_regex: Regex::new(r"(?i)work\s+time\s+(\d+)\s+(minutes?|hours?)")?,
            holiday_regex: Regex::new(r"(?i)(stat(?:utory)?\s+holiday|pto|holiday\s+day)")?,
        })
    }

    fn parse_file(&self, content: &str, date: NaiveDate) -> Result<DaySummary, Box<dyn std::error::Error>> {
        let mut entries = Vec::new();
        let mut current_entry = TimeEntry::new();
        let mut total_work_time_duration = Duration::zero();

        for line in content.lines() {
            if let Some(caps) = self.start_regex.captures(line) {
                if current_entry.start_time.is_some() {
                    entries.push(current_entry);
                    current_entry = TimeEntry::new();
                }

                let hours: u32 = caps[1].parse()?;
                let minutes: u32 = caps[2].parse()?;
                
                if let Some(time) = NaiveTime::from_hms_opt(hours, minutes, 0) {
                    current_entry.start_time = Some(time);
                }
            } else if let Some(caps) = self.stop_regex.captures(line) {
                let hours: u32 = caps[1].parse()?;
                let minutes: u32 = caps[2].parse()?;
                
                if let Some(time) = NaiveTime::from_hms_opt(hours, minutes, 0) {
                    current_entry.end_time = Some(time);
                    entries.push(current_entry);
                    current_entry = TimeEntry::new();
                }
            } else if let Some(caps) = self.work_time_regex.captures(line) {
                let amount: u32 = caps[1].parse()?;
                let unit = caps[2].to_lowercase();
                
                let duration = if unit.starts_with("hour") {
                    Duration::hours(amount as i64)
                } else if unit.starts_with("minute") {
                    Duration::minutes(amount as i64)
                } else {
                    Duration::zero()
                };
                
                total_work_time_duration += duration;
            } else if self.holiday_regex.is_match(line) {
                total_work_time_duration += Duration::hours(8);
            }
        }

        if current_entry.start_time.is_some() {
            entries.push(current_entry);
        }

        let time_entries_duration: Duration = entries
            .iter()
            .filter_map(|entry| entry.duration())
            .sum();
        
        let total_duration = time_entries_duration + total_work_time_duration;

        Ok(DaySummary {
            date,
            total_duration,
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

    fn group_by_week(&self, summaries: &[DaySummary]) -> Vec<WeekSummary> {
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

    fn group_by_month(&self, summaries: &[DaySummary]) -> Vec<MonthlySummary> {
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
    format!("{}h {:02}m", hours, minutes)
}

const MONTH_NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December"
];

fn get_month_name(month: u32) -> &'static str {
    MONTH_NAMES.get(month.saturating_sub(1) as usize).map_or("Unknown", |&name| name)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    let mut directory = ".";
    let mut weekly_hours = 40.0;
    
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
            "--help" | "-h" => {
                println!("Usage: {} [directory] [--weekly-hours HOURS]", args[0]);
                println!("  directory: Directory containing markdown timesheet files (default: current directory)");
                println!("  --weekly-hours: Expected weekly work hours (default: 40)");
                return Ok(());
            }
            _ => {
                directory = &args[i];
                i += 1;
            }
        }
    }

    let parser = TimesheetParser::new()?;
    let summaries = parser.parse_directory(Path::new(directory))?;
    let weeks = parser.group_by_week(&summaries);
    let months = parser.group_by_month(&summaries);

    // Calculate the date two weeks ago from today
    let today = chrono::Local::now().date_naive();
    let two_weeks_ago = today - Duration::days(14);

    println!("Daily Summary (Last 2 Weeks):");
    println!("==============================");
    weeks
        .iter()
        .flat_map(|week| &week.days)
        .filter(|day| day.total_duration > Duration::zero() && day.date >= two_weeks_ago)
        .for_each(|day| {
            let weekday = day.date.format("%a");
            println!("{} {:3} - {}", day.date, weekday, format_duration(day.total_duration));
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
            let actual_hours = week.total_duration.num_minutes() as f64 / 60.0;
            let formatted_duration = format_duration(week.total_duration);
            
            if actual_hours < weekly_hours {
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
        let parser = TimesheetParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_entry() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Start work 9:00\nSome notes\nStop work 17:30";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.date, date);
        assert_eq!(summary.total_duration.num_hours(), 8);
        assert_eq!(summary.total_duration.num_minutes() % 60, 30);
    }

    #[test]
    fn test_parse_multiple_entries() {
        let parser = TimesheetParser::new().unwrap();
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
        let parser = TimesheetParser::new().unwrap();
        let content = "START WORK 9:00\nstop Work 17:30";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_parse_incomplete_entry() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Start work 9:00\nSome work done but forgot to stop";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration, Duration::zero());
    }

    #[test]
    fn test_parse_invalid_times() {
        let parser = TimesheetParser::new().unwrap();
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
        let parser = TimesheetParser::new().unwrap();
        let summaries = vec![
            DaySummary {
                date: NaiveDate::from_ymd_opt(2025, 8, 25).unwrap(), // Monday
                total_duration: Duration::hours(8),
            },
            DaySummary {
                date: NaiveDate::from_ymd_opt(2025, 8, 26).unwrap(), // Tuesday
                total_duration: Duration::hours(7),
            },
            DaySummary {
                date: NaiveDate::from_ymd_opt(2025, 9, 1).unwrap(), // Next Monday
                total_duration: Duration::hours(6),
            },
        ];

        let weeks = parser.group_by_week(&summaries);
        assert_eq!(weeks.len(), 2);
        
        assert_eq!(weeks[0].days.len(), 2);
        assert_eq!(weeks[0].total_duration.num_hours(), 15);
        
        assert_eq!(weeks[1].days.len(), 1);
        assert_eq!(weeks[1].total_duration.num_hours(), 6);
    }

    #[test]
    fn test_overlapping_entries() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Start work 9:00\nStart work 10:00\nStop work 17:00";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 7);
    }

    #[test]
    fn test_military_time_formats() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Start work 8:15\nStop work 16:45";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
        assert_eq!(summary.total_duration.num_minutes() % 60, 30);
    }

    #[test]
    fn test_single_digit_hours() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Start work 9:00\nStop work 5:00";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 20);
    }

    #[test]
    fn test_empty_file() {
        let parser = TimesheetParser::new().unwrap();
        let content = "";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration, Duration::zero());
    }

    #[test]
    fn test_no_matching_lines() {
        let parser = TimesheetParser::new().unwrap();
        let content = "# Daily Notes\n\nWorked on project today.\nHad meetings.";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration, Duration::zero());
    }

    #[test]
    fn test_parse_different_formats() {
        let parser = TimesheetParser::new().unwrap();
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
        let parser = TimesheetParser::new().unwrap();
        let content = "Work time 90 minutes read textbook";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 1);
        assert_eq!(summary.total_duration.num_minutes() % 60, 30);
    }

    #[test]
    fn test_work_time_hour() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Work time 1 hour did other work";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 1);
        assert_eq!(summary.total_duration.num_minutes() % 60, 0);
    }

    #[test]
    fn test_work_time_hours_plural() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Work time 3 hours completed project";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 3);
        assert_eq!(summary.total_duration.num_minutes() % 60, 0);
    }

    #[test]
    fn test_work_time_mixed_with_start_stop() {
        let parser = TimesheetParser::new().unwrap();
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
        let parser = TimesheetParser::new().unwrap();
        let content = "WORK TIME 45 MINUTES testing";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_minutes(), 45);
    }

    #[test]
    fn test_stat_holiday() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Stat holiday";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_statutory_holiday() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Statutory holiday";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_case_insensitive() {
        let parser = TimesheetParser::new().unwrap();
        let content = "STAT HOLIDAY";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_with_context() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Today was a stat holiday - Labour Day";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_mixed_with_other_entries() {
        let parser = TimesheetParser::new().unwrap();
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
        let parser = TimesheetParser::new().unwrap();
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
        let parser = TimesheetParser::new().unwrap();
        let content = "PTO";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_pto_case_insensitive() {
        let parser = TimesheetParser::new().unwrap();
        let content = "pto";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_pto_with_context() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Taking PTO today for vacation";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_day() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Holiday day";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_day_case_insensitive() {
        let parser = TimesheetParser::new().unwrap();
        let content = "HOLIDAY DAY";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_holiday_day_with_context() {
        let parser = TimesheetParser::new().unwrap();
        let content = "Christmas is a holiday day";
        let date = NaiveDate::from_ymd_opt(2025, 8, 25).unwrap();

        let summary = parser.parse_file(content, date).unwrap();
        assert_eq!(summary.total_duration.num_hours(), 8);
    }

    #[test]
    fn test_mixed_holiday_types() {
        let parser = TimesheetParser::new().unwrap();
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
}