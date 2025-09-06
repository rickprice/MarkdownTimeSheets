# Markdown TimeSheet

A command-line tool for parsing time tracking data from markdown files and generating daily, weekly, and monthly summaries with configurable weekly hour targets and shortage tracking.

## Overview

This tool parses markdown files containing time entries in natural language format and calculates total hours worked per day, week, and month. It's designed to work with simple markdown files where you log your work start/stop times, direct work time entries, and holidays. The tool can track weekly hour shortages against configurable targets.

## Features

- **Natural Language Parsing**: Recognizes various formats like "Start work 9:00", "Started working at 8:30", "Stop work 17:30"
- **Case Insensitive**: Works with different capitalizations
- **Multiple Entries**: Supports multiple work sessions per day (e.g., breaks for lunch)
- **Overnight Support**: Handles work sessions that cross midnight
- **Direct Time Entries**: Supports "Work time X hours/minutes" for flexible logging
- **Holiday Support**: Automatically adds 8 hours for holidays, PTO, and statutory holidays
- **Configurable Weekly Hours**: Set custom weekly hour targets (default: 40 hours)
- **Hour Shortage Tracking**: Shows how many hours short when weekly target isn't met
- **Daily Summaries**: Shows total hours worked each day (filtered to last 2 weeks)
- **Weekly Summaries**: Groups days by week with shortage indicators
- **Monthly Summaries**: Shows total hours by month
- **Flexible Time Format**: Supports both 12-hour and 24-hour time formats

## Usage

Run the tool on a directory containing markdown files named in `YYYY-MM-DD.md` format:

```bash
# Basic usage (scans current directory, 40-hour weeks)
cargo run

# Specify directory
cargo run /path/to/timesheets

# Set custom weekly hours target
cargo run --weekly-hours 37.5

# Combined options
cargo run /path/to/timesheets --weekly-hours 35

# Show help
cargo run -- --help
```

### Command Line Options

- `directory`: Directory containing markdown timesheet files (default: current directory)
- `--weekly-hours HOURS`: Expected weekly work hours for shortage calculation (default: 40)
- `--help`, `-h`: Show usage information

## File Format

Create markdown files with names like `2025-08-25.md` containing time entries:

```markdown
# Daily Notes

Start work 9:00
Had a productive morning working on the project.

Stop work 12:00

Lunch break

Start work 13:00
Afternoon session focused on testing.

Stop work 17:30

# Alternative: Direct time logging
Work time 2 hours code review
Work time 30 minutes documentation

# Holiday/PTO examples
Stat holiday
PTO
Holiday day
```

## Supported Time Entry Formats

The parser recognizes these patterns (case insensitive):

### Start/Stop Time Entries
- `Start work 9:00`
- `Started working at 8:30`
- `Stop work 17:30`
- `Stopped working at 16:45`

### Direct Time Entries
- `Work time 2 hours project work`
- `Work time 30 minutes meeting`
- `Work time 1 hour documentation`
- `Work time 90 minutes code review`

### Holiday/PTO Entries (automatically adds 8 hours)
- `Stat holiday` or `Statutory holiday`
- `PTO`
- `Holiday day`

## Installation

1. Clone the repository
2. Run `cargo build --release`
3. The binary will be available at `target/release/markdown_timesheet`

## Requirements

- Rust 2021 edition or later
- Dependencies: chrono, regex

## Example Output

```
Daily Summary (Last 2 Weeks):
==============================
2025-08-25 Mon - 8h 30m
2025-08-26 Tue - 7h 00m  
2025-08-27 Wed - 6h 15m
2025-08-28 Thu - 8h 00m
2025-08-29 Fri - 4h 15m

Monthly Summary:
================
August 2025: 156h 45m
September 2025: 42h 30m

Weekly Summary:
===============
Week of 2025-08-25 - 2025-08-31: 34h 00m [6h 00m short]
Week of 2025-09-01 - 2025-09-07: 42h 30m
```

The weekly summary shows shortages when using the default 40-hour target. Weeks that meet or exceed the target show no shortage indicator.

## Testing

Run tests with:

```bash
cargo test
```

## Code Quality

The codebase follows Rust best practices and passes all quality checks:

```bash
# Run clippy for linting
cargo clippy -- -D warnings

# Run tests
cargo test

# Build optimized release
cargo build --release
```

## Contributing

The code is optimized for performance and follows canonical Rust patterns:
- Uses iterator chains with `sum()` and `map()` for efficiency
- Employs `sort_unstable_by_key` for better performance
- Avoids unnecessary allocations and `unwrap()` calls
- Passes clippy with zero warnings on strict mode