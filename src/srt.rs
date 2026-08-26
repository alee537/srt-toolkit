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
