// WebVTT input parsing.
//
// WebVTT and SubRip agree on the shape that matters here - a timing line
// with "-->" followed by one or more lines of text, blocks separated by
// blank lines - so this parser produces the same format-agnostic `Cue` and
// `ParseError` types `srt` does. The differences are: a required "WEBVTT"
// signature line, an optional non-numeric cue identifier before the timing
// line, "." instead of "," before milliseconds, an optional hours field,
// and optional cue settings (e.g. "align:start") trailing the timing line.
// A cue identifier is kept on `Cue::identifier` so JSON output can report the
// original label, but the normalized text output still renumbers cues
// sequentially like SubRip does - a plain .vtt file has no place to put a
// name alongside the numbering scheme `format` produces.
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

        let mut identifier: Option<String> = None;
        let mut timing_line_no = i + 1;
        let mut timing_line = lines[i].trim();
        if !timing_line.contains("-->") {
            // This line is a cue identifier; the timing line follows it.
            identifier = Some(timing_line.to_string());
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
            identifier,
        });
        next_number += 1;
    }

    (cues, errors)
}

/// Rewrites cues as a normalized WebVTT document: the required "WEBVTT"
/// signature, sequential cue identifiers, zero-padded timestamps with the
/// '.' millisecond separator, LF line endings, one blank line between cues.
pub fn format(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");

    for (position, cue) in cues.iter().enumerate() {
        out.push_str(&(position + 1).to_string());
        out.push('\n');
        out.push_str(&cue.start.format_vtt());
        out.push_str(" --> ");
        out.push_str(&cue.end.format_vtt());
        out.push('\n');
        for line in &cue.text {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }

    out
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start_seconds: u32, end_seconds: u32, text: &str) -> Cue {
        Cue {
            number: 1,
            start: Timecode { hours: 0, minutes: 0, seconds: start_seconds, millis: 0 },
            end: Timecode { hours: 0, minutes: 0, seconds: end_seconds, millis: 0 },
            text: vec![text.to_string()],
            identifier: None,
        }
    }

    #[test]
    fn format_starts_with_the_webvtt_signature() {
        let out = format(&[cue(1, 4, "hi")]);
        assert!(out.starts_with("WEBVTT\n\n"));
    }

    #[test]
    fn format_uses_dot_separated_timecodes_and_sequential_identifiers() {
        let out = format(&[cue(1, 4, "first"), cue(5, 7, "second")]);
        assert_eq!(
            out,
            "WEBVTT\n\n1\n00:00:01.000 --> 00:00:04.000\nfirst\n\n2\n00:00:05.000 --> 00:00:07.000\nsecond\n\n"
        );
    }

    #[test]
    fn format_renumbers_regardless_of_input_cue_numbers() {
        let mut cues = vec![cue(1, 4, "first")];
        cues[0].number = 42;
        let out = format(&cues);
        assert!(out.contains("42") == false);
        assert!(out.contains("\n1\n"));
    }

    #[test]
    fn parse_captures_a_named_cue_identifier() {
        let (cues, errors) = parse(
            "WEBVTT\n\nintro\n00:00:01.000 --> 00:00:04.000\nHello there.\n",
        );
        assert!(errors.is_empty());
        assert_eq!(cues[0].identifier, Some("intro".to_string()));
    }

    #[test]
    fn parse_leaves_identifier_none_when_absent() {
        let (cues, errors) = parse("WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello there.\n");
        assert!(errors.is_empty());
        assert_eq!(cues[0].identifier, None);
    }

    #[test]
    fn format_does_not_write_identifiers_into_the_text_output() {
        let mut cue = cue(1, 4, "hi");
        cue.identifier = Some("intro".to_string());
        let out = format(&[cue]);
        assert!(!out.contains("intro"));
    }

    #[test]
    fn format_round_trips_through_parse() {
        let (cues, errors) = parse("WEBVTT\n\n1\n00:00:01.000 --> 00:00:04.000\nHello there.\n");
        assert!(errors.is_empty());
        let out = format(&cues);
        let (reparsed, errors) = parse(&out);
        assert!(errors.is_empty());
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].start, cues[0].start);
        assert_eq!(reparsed[0].end, cues[0].end);
        assert_eq!(reparsed[0].text, cues[0].text);
    }
}
