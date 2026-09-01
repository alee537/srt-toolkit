// WebVTT input parsing.
//
// WebVTT and SubRip agree on the shape that matters here - a timing line
// with "-->" followed by one or more lines of text, blocks separated by
// blank lines - so this parser produces the same format-agnostic `Cue` and
// `ParseError` types `srt` does. The differences are: a required "WEBVTT"
// signature line, an optional non-numeric cue identifier before the timing
// line, "." instead of "," before milliseconds, an optional hours field,
// and optional cue settings (e.g. "align:start") trailing the timing line.
// Cue identifiers aren't kept - like SubRip cue numbers, they're renumbered
// sequentially on output, so there's nothing format-specific left to carry
// through `Cue`.
//
// NOTE and STYLE blocks are recognized and skipped rather than treated as
// malformed cues.

use crate::srt::{Cue, ParseError, Timecode};

pub fn parse(input: &str) -> (Vec<Cue>, Vec<ParseError>) {
    let normalized = input.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();

    let mut cues = Vec::new();
    let mut errors = Vec::new();
    let mut i = 0;

    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }

    if i >= lines.len() || !lines[i].trim_start().starts_with("WEBVTT") {
        errors.push(ParseError {
            line: i + 1,
            message: "file does not start with a WEBVTT signature".to_string(),
        });
        return (cues, errors);
    }
    i += 1;

    let mut next_number = 1u32;

    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        let first = lines[i].trim();
        if first.starts_with("NOTE") || first.starts_with("STYLE") {
            i = skip_to_blank(&lines, i);
            continue;
        }

        let mut timing_line_no = i + 1;
        let mut timing_line = lines[i].trim();
        if !timing_line.contains("-->") {
            // This line is a cue identifier; the timing line follows it.
            i += 1;
            if i >= lines.len() {
                errors.push(ParseError {
                    line: timing_line_no,
                    message: "cue identifier is not followed by a timing line".to_string(),
                });
                break;
            }
            timing_line_no = i + 1;
            timing_line = lines[i].trim();
        }

        let (start_raw, rest) = match timing_line.split_once("-->") {
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
        // Cue settings (e.g. "align:start line:0%") trail the end timecode,
        // separated by whitespace.
        let end_raw = rest.split_whitespace().next().unwrap_or(rest);
        i += 1;

        let start = parse_timecode(start_raw);
        let end = parse_timecode(end_raw);
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
                line: timing_line_no,
                message: format!("cue at line {} has a timing line but no text", timing_line_no),
            });
        }

        cues.push(Cue {
            number: next_number,
            start,
            end,
            text,
        });
        next_number += 1;
    }

    (cues, errors)
}

fn skip_to_blank(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() && !lines[i].trim().is_empty() {
        i += 1;
    }
    i
}

fn parse_timecode(raw: &str) -> Result<Timecode, String> {
    let (time_part, millis_part) = raw
        .split_once('.')
        .ok_or_else(|| format!("timecode '{}' is missing the '.mmm' millisecond part", raw))?;

    let fields: Vec<&str> = time_part.split(':').collect();
    let (hours, minutes, seconds) = match fields.as_slice() {
        [h, m, s] => (
            h.parse::<u32>()
                .map_err(|_| format!("invalid hours in timecode '{}'", raw))?,
            m.parse::<u32>()
                .map_err(|_| format!("invalid minutes in timecode '{}'", raw))?,
            s.parse::<u32>()
                .map_err(|_| format!("invalid seconds in timecode '{}'", raw))?,
        ),
        [m, s] => (
            0,
            m.parse::<u32>()
                .map_err(|_| format!("invalid minutes in timecode '{}'", raw))?,
            s.parse::<u32>()
                .map_err(|_| format!("invalid seconds in timecode '{}'", raw))?,
        ),
        _ => return Err(format!("timecode '{}' must look like [HH:]MM:SS.mmm", raw)),
    };
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
