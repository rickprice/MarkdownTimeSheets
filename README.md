# Markdown TimeSheet

A command-line tool for parsing time tracking data from markdown files and generating daily and weekly summaries.

## Overview

This tool parses markdown files containing time entries in natural language format and calculates total hours worked per day and week. It's designed to work with simple markdown files where you log your work start and stop times.

## Features

- **Natural Language Parsing**: Recognizes various formats like "Start work 9:00", "Started working at 8:30", "Stop work 17:30"
- **Case Insensitive**: Works with different capitalizations
- **Multiple Entries**: Supports multiple work sessions per day (e.g., breaks for lunch)
- **Overnight Support**: Handles work sessions that cross midnight
- **Daily Summaries**: Shows total hours worked each day
- **Weekly Summaries**: Groups days by week and shows weekly totals
- **Flexible Time Format**: Supports both 12-hour and 24-hour time formats

## Usage

Run the tool on a directory containing markdown files named in `YYYY-MM-DD.md` format:

```bash
cargo run [directory]
```

If no directory is specified, it will scan the current directory.

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
```

## Supported Time Entry Formats

The parser recognizes these patterns (case insensitive):
- `Start work 9:00`
- `Started working at 8:30`
- `Stop work 17:30`
- `Stopped working at 16:45`

## Installation

1. Clone the repository
2. Run `cargo build --release`
3. The binary will be available at `target/release/markdown_timesheet`

## Requirements

- Rust 2021 edition or later
- Dependencies: chrono, regex

## Example Output

```
Daily Summary:
==============
2025-08-25: 8h 30m
2025-08-26: 7h 00m
2025-09-01: 6h 15m

Weekly Summary:
===============
Week of 2025-08-25 - 2025-08-31: 15h 30m
Week of 2025-09-01 - 2025-09-07: 6h 15m
```

## Testing

Run tests with:

```bash
cargo test
```