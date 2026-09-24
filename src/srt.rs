// SubRip (.srt) parsing, validation, and pretty-printing.
//
// A file is a sequence of blocks separated by one or more blank lines:
//
//   1
//   00:00:01,000 --> 00:00:04,000
//   Hello there.
//
//   2
//   00:00:05,250 --> 00:00:07,000
//   Second line of the cue.
//
// There's no real spec, just decades of players agreeing on this shape by
// convention, so the parser is stricter than most players in order to be
// useful as a linter, but it still resyncs after a bad block instead of
// giving up on the whole file.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timecode {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub millis: u32,
}

impl Timecode {
    pub fn to_millis(&self) -> u64 {
        (self.hours as u64) * 3_600_000
            + (self.minutes as u64) * 60_000
            + (self.seconds as u64) * 1_000
            + self.millis as u64
    }

    pub fn format(&self) -> String {
        format!(
            "{:02}:{:02}:{:02},{:03}",
            self.hours, self.minutes, self.seconds, self.millis
        )
    }

    /// Same as `format`, but with the '.' millisecond separator WebVTT uses
    /// instead of SubRip's ','.
    pub fn format_vtt(&self) -> String {
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            self.hours, self.minutes, self.seconds, self.millis
        )
    }

    pub fn from_millis(total: u64) -> Timecode {
        let millis = (total % 1_000) as u32;
        let total_seconds = total / 1_000;
        let seconds = (total_seconds % 60) as u32;
        let total_minutes = total_seconds / 60;
        let minutes = (total_minutes % 60) as u32;
        let hours = (total_minutes / 60) as u32;
        Timecode {
            hours,
            minutes,
            seconds,
            millis,
        }
    }

    fn parse(raw: &str) -> Result<Timecode, String> {
        let (time_part, millis_part) = raw
            .split_once(',')
            .ok_or_else(|| format!("timecode '{}' is missing the ',mmm' millisecond part", raw))?;

        let fields: Vec<&str> = time_part.split(':').collect();
        if fields.len() != 3 {
            return Err(format!("timecode '{}' must look like HH:MM:SS,mmm", raw));
        }

        let hours = fields[0]
            .parse::<u32>()
            .map_err(|_| format!("invalid hours in timecode '{}'", raw))?;
        let minutes = fields[1]
            .parse::<u32>()
            .map_err(|_| format!("invalid minutes in timecode '{}'", raw))?;
        let seconds = fields[2]
            .parse::<u32>()
            .map_err(|_| format!("invalid seconds in timecode '{}'", raw))?;
        let millis = millis_part
            .parse::<u32>()
            .map_err(|_| format!("invalid milliseconds in timecode '{}'", raw))?;

        if minutes >= 60 {
            return Err(format!("minutes must be 0-59 in timecode '{}'", raw));
        }
        if seconds >= 60 {
            return Err(format!("seconds must be 0-59 in timecode '{}'", raw));
        }
        if millis > 999 {
            return Err(format!("milliseconds must be 0-999 in timecode '{}'", raw));
        }

        Ok(Timecode {
            hours,
            minutes,
            seconds,
            millis,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Cue {
    pub number: u32,
    pub start: Timecode,
    pub end: Timecode,
    pub text: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub cue_number: u32,
    pub message: String,
}

/// Parses raw file contents into cues, collecting parse errors along the
/// way instead of stopping at the first one. A block that fails to parse
/// is skipped and the reader resyncs at the next blank line.
pub fn parse(input: &str) -> (Vec<Cue>, Vec<ParseError>) {
    let normalized = input.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();

    let mut cues = Vec::new();
    let mut errors = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        let number_line_no = i + 1;
        let number: u32 = match lines[i].trim().parse() {
            Ok(n) => n,
            Err(_) => {
                errors.push(ParseError {
                    line: number_line_no,
                    message: format!("expected a cue number, found '{}'", lines[i].trim()),
                });
                i = skip_to_blank(&lines, i);
                continue;
            }
        };
        i += 1;

        if i >= lines.len() {
            errors.push(ParseError {
                line: number_line_no + 1,
                message: "file ends after a cue number, expected a timing line".to_string(),
            });
            break;
        }

        let timing_line_no = i + 1;
        let timing_line = lines[i].trim();
        let (start_raw, end_raw) = match timing_line.split_once("-->") {
            Some((s, e)) => (s.trim(), e.trim()),
            None => {
                errors.push(ParseError {
                    line: timing_line_no,
                    message: format!("expected a timing line with '-->', found '{}'", timing_line),
                });
                i = skip_to_blank(&lines, i);
                continue;
            }
        };
        i += 1;

        let start = Timecode::parse(start_raw);
        let end = Timecode::parse(end_raw);
        let (start, end) = match (start, end) {
            (Ok(s), Ok(e)) => (s, e),
            (s, e) => {
                if let Err(message) = s {
                    errors.push(ParseError { line: timing_line_no, message });
                }
                if let Err(message) = e {
                    errors.push(ParseError { line: timing_line_no, message });
                }
                i = skip_to_blank(&lines, i);
                continue;
            }
        };

        let mut text = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            text.push(lines[i].trim_end().to_string());
            i += 1;
        }

        if text.is_empty() {
            errors.push(ParseError {
                line: number_line_no,
                message: format!("cue {} has a timing line but no text", number),
            });
        }

        cues.push(Cue {
            number,
            start,
            end,
            text,
        });
    }

    (cues, errors)
}

fn skip_to_blank(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() && !lines[i].trim().is_empty() {
        i += 1;
    }
    i
}

/// Semantic checks that only make sense once every cue has parsed cleanly:
/// ordering, overlap, and duplicate numbering. Structural problems belong
/// in `ParseError`, not here.
pub fn validate(cues: &[Cue]) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut seen_numbers = std::collections::HashSet::new();
    let mut previous_end: Option<u64> = None;

    for cue in cues {
        if cue.start.to_millis() >= cue.end.to_millis() {
            issues.push(Issue {
                cue_number: cue.number,
                message: "start time is not before end time".to_string(),
            });
        }

        if let Some(prev_end) = previous_end {
            if cue.start.to_millis() < prev_end {
                issues.push(Issue {
                    cue_number: cue.number,
                    message: "starts before the previous cue ends".to_string(),
                });
            }
        }

        if !seen_numbers.insert(cue.number) {
            issues.push(Issue {
                cue_number: cue.number,
                message: "duplicate cue number".to_string(),
            });
        }

        previous_end = Some(cue.end.to_millis());
    }

    issues
}

/// Flags gaps in cue numbering - a cue whose number isn't one more than the
/// previous cue's. Separate from `validate` because most players don't care
/// about numbering at all (they use timing to find the cue to display), so
/// this is opt-in via `--strict` rather than a default issue.
pub fn validate_numbering(cues: &[Cue]) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut expected: Option<u32> = None;

    for cue in cues {
        if let Some(exp) = expected {
            if cue.number != exp {
                issues.push(Issue {
                    cue_number: cue.number,
                    message: format!("numbering gap: expected {}, found {}", exp, cue.number),
                });
            }
        }
        expected = Some(cue.number + 1);
    }

    issues
}

#[derive(Debug, Clone)]
pub struct Fix {
    pub cue_number: u32,
    pub message: String,
}

/// Adjusts cue timings in place to remove the two timing problems
/// `validate` flags that have an unambiguous mechanical fix: an overlap
/// with the previous cue, and a non-positive duration. Duplicate numbering
/// isn't handled here - `format` always renumbers sequentially, so by the
/// time a fixed file is written there's nothing left to fix.
///
/// Fixes are applied in cue order, and each fix feeds into the next: an
/// overlap fix moves `previous_end` forward, which can turn a later cue's
/// start into a new overlap, so cascading pushes resolve in one pass.
pub fn fix(cues: &mut [Cue]) -> Vec<Fix> {
    let mut fixes = Vec::new();
    let mut previous_end: Option<u64> = None;

    for cue in cues.iter_mut() {
        if let Some(prev_end) = previous_end {
            if cue.start.to_millis() < prev_end {
                cue.start = Timecode::from_millis(prev_end);
                fixes.push(Fix {
                    cue_number: cue.number,
                    message: "moved start to the end of the previous cue to remove overlap"
                        .to_string(),
                });
            }
        }

        if cue.start.to_millis() >= cue.end.to_millis() {
            cue.end = Timecode::from_millis(cue.start.to_millis() + 1);
            fixes.push(Fix {
                cue_number: cue.number,
                message: "extended end time to be after start time".to_string(),
            });
        }

        previous_end = Some(cue.end.to_millis());
    }

    fixes
}

/// Rewrites cues as a normalized .srt document: sequential numbering,
/// zero-padded timestamps, LF line endings, one blank line between cues.
pub fn format(cues: &[Cue]) -> String {
    let mut out = String::new();

    for (position, cue) in cues.iter().enumerate() {
        out.push_str(&(position + 1).to_string());
        out.push('\n');
        out.push_str(&cue.start.format());
        out.push_str(" --> ");
        out.push_str(&cue.end.format());
        out.push('\n');
        for line in &cue.text {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_timecode() {
        let tc = Timecode::parse("01:02:03,456").unwrap();
        assert_eq!(tc.hours, 1);
        assert_eq!(tc.minutes, 2);
        assert_eq!(tc.seconds, 3);
        assert_eq!(tc.millis, 456);
    }

    #[test]
    fn parses_zero_timecode() {
        let tc = Timecode::parse("00:00:00,000").unwrap();
        assert_eq!(tc.to_millis(), 0);
    }

    #[test]
    fn accepts_hours_past_24() {
        // Nothing in the format caps hours at 24 - a long compilation reel
        // can legitimately run past a day of accumulated runtime.
        let tc = Timecode::parse("30:00:00,000").unwrap();
        assert_eq!(tc.hours, 30);
    }

    #[test]
    fn rejects_minutes_at_60() {
        assert!(Timecode::parse("00:60:00,000").is_err());
    }

    #[test]
    fn rejects_seconds_at_60() {
        assert!(Timecode::parse("00:00:60,000").is_err());
    }

    #[test]
    fn rejects_millis_over_999() {
        assert!(Timecode::parse("00:00:00,1000").is_err());
    }

    #[test]
    fn accepts_millis_at_max() {
        let tc = Timecode::parse("00:00:00,999").unwrap();
        assert_eq!(tc.millis, 999);
    }

    #[test]
    fn rejects_missing_comma() {
        assert!(Timecode::parse("00:00:00.500").is_err());
    }

    #[test]
    fn rejects_wrong_field_count() {
        assert!(Timecode::parse("00:00,000").is_err());
        assert!(Timecode::parse("00:00:00:00,000").is_err());
    }

    #[test]
    fn rejects_non_numeric_fields() {
        assert!(Timecode::parse("aa:00:00,000").is_err());
        assert!(Timecode::parse("00:00:00,abc").is_err());
    }

    #[test]
    fn to_millis_round_trip() {
        let tc = Timecode {
            hours: 2,
            minutes: 15,
            seconds: 37,
            millis: 89,
        };
        let total = tc.to_millis();
        assert_eq!(total, 8_137_089);
        assert_eq!(Timecode::from_millis(total), tc);
    }

    #[test]
    fn from_millis_rolls_over_seconds_and_minutes() {
        // 1 minute, 0 seconds, 0 millis - carries should propagate cleanly
        // across every field boundary at once.
        let tc = Timecode::from_millis(60_000);
        assert_eq!(tc.minutes, 1);
        assert_eq!(tc.seconds, 0);
        assert_eq!(tc.millis, 0);
    }

    #[test]
    fn from_millis_zero() {
        let tc = Timecode::from_millis(0);
        assert_eq!(tc, Timecode { hours: 0, minutes: 0, seconds: 0, millis: 0 });
    }

    #[test]
    fn format_zero_pads_every_field() {
        let tc = Timecode { hours: 1, minutes: 2, seconds: 3, millis: 4 };
        assert_eq!(tc.format(), "01:02:03,004");
    }

    #[test]
    fn format_does_not_truncate_hours_past_two_digits() {
        let tc = Timecode { hours: 100, minutes: 0, seconds: 0, millis: 0 };
        assert_eq!(tc.format(), "100:00:00,000");
    }

    #[test]
    fn format_vtt_uses_a_dot_before_milliseconds() {
        let tc = Timecode { hours: 1, minutes: 2, seconds: 3, millis: 4 };
        assert_eq!(tc.format_vtt(), "01:02:03.004");
    }

    fn cue(number: u32) -> Cue {
        Cue {
            number,
            start: Timecode { hours: 0, minutes: 0, seconds: 0, millis: 0 },
            end: Timecode { hours: 0, minutes: 0, seconds: 1, millis: 0 },
            text: vec!["x".to_string()],
        }
    }

    #[test]
    fn numbering_gap_none_for_sequential_cues() {
        let cues = vec![cue(1), cue(2), cue(3)];
        assert!(validate_numbering(&cues).is_empty());
    }

    #[test]
    fn numbering_gap_flags_skipped_number() {
        let cues = vec![cue(1), cue(3)];
        let issues = validate_numbering(&cues);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].cue_number, 3);
        assert_eq!(issues[0].message, "numbering gap: expected 2, found 3");
    }

    #[test]
    fn numbering_gap_allows_starting_above_one() {
        // Nothing requires the first cue to be numbered 1 - only that
        // numbering is sequential from wherever it starts.
        let cues = vec![cue(5), cue(6)];
        assert!(validate_numbering(&cues).is_empty());
    }

    #[test]
    fn numbering_gap_flags_duplicate_as_a_gap_too() {
        let cues = vec![cue(1), cue(1)];
        let issues = validate_numbering(&cues);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].message, "numbering gap: expected 2, found 1");
    }

    #[test]
    fn numbering_gap_flags_backwards_numbering() {
        let cues = vec![cue(2), cue(1)];
        let issues = validate_numbering(&cues);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].message, "numbering gap: expected 3, found 1");
    }
}
