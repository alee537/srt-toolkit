use std::env;
use std::fs;
use std::io::{self, Read};
use std::process;

mod json;
mod srt;
mod vtt;

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args.first().map(String::as_str).unwrap_or("srt-toolkit");

    if args.len() < 2 {
        print_usage(program);
        process::exit(2);
    }

    let command = args[1].as_str();
    let mut json_mode = false;
    let mut fix_mode = false;
    let mut strict_mode = false;
    let mut format_override: Option<&str> = None;
    let mut output_format: Option<&str> = None;
    let mut path: Option<&str> = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_mode = true;
                i += 1;
            }
            "--fix" => {
                fix_mode = true;
                i += 1;
            }
            "--strict" => {
                strict_mode = true;
                i += 1;
            }
            "--format" => {
                i += 1;
                let value = match args.get(i) {
                    Some(v) => v.as_str(),
                    None => {
                        eprintln!("--format requires a value ('srt' or 'vtt')");
                        process::exit(2);
                    }
                };
                match value {
                    "srt" | "vtt" => format_override = Some(value),
                    other => {
                        eprintln!("unknown format '{}', expected 'srt' or 'vtt'", other);
                        process::exit(2);
                    }
                }
                i += 1;
            }
            "--to" => {
                i += 1;
                let value = match args.get(i) {
                    Some(v) => v.as_str(),
                    None => {
                        eprintln!("--to requires a value ('srt' or 'vtt')");
                        process::exit(2);
                    }
                };
                match value {
                    "srt" | "vtt" => output_format = Some(value),
                    other => {
                        eprintln!("unknown output format '{}', expected 'srt' or 'vtt'", other);
                        process::exit(2);
                    }
                }
                i += 1;
            }
            other if path.is_none() => {
                path = Some(other);
                i += 1;
            }
            other => {
                eprintln!("unexpected argument '{}'", other);
                process::exit(2);
            }
        }
    }

    if fix_mode && command != "format" {
        eprintln!("--fix is only valid with the 'format' command");
        process::exit(2);
    }

    if strict_mode && command != "validate" {
        eprintln!("--strict is only valid with the 'validate' command");
        process::exit(2);
    }

    if output_format.is_some() && command != "format" {
        eprintln!("--to is only valid with the 'format' command");
        process::exit(2);
    }

    let path = match path {
        Some(p) => p,
        None => {
            print_usage(program);
            process::exit(2);
        }
    };

    // "-" reads from stdin, same convention as most other line-oriented
    // Unix tools. Stdin has no file extension to sniff, so pass along
    // "<stdin>" as the display name and require --format for .vtt input.
    let (contents, display_name) = if path == "-" {
        let mut buf = String::new();
        match io::stdin().read_to_string(&mut buf) {
            Ok(_) => (buf, "<stdin>".to_string()),
            Err(e) => {
                eprintln!("could not read from stdin: {}", e);
                process::exit(1);
            }
        }
    } else {
        match fs::read_to_string(path) {
            Ok(c) => (c, path.to_string()),
            Err(e) => {
                eprintln!("could not read '{}': {}", path, e);
                process::exit(1);
            }
        }
    };

    match command {
        "validate" => run_validate(
            &display_name,
            &contents,
            json_mode,
            strict_mode,
            format_override,
        ),
        "format" => run_format(
            &display_name,
            &contents,
            json_mode,
            fix_mode,
            format_override,
            output_format,
        ),
        other => {
            eprintln!("unknown command '{}'", other);
            print_usage(program);
            process::exit(2);
        }
    }
}

fn print_usage(program: &str) {
    eprintln!("usage:");
    eprintln!(
        "  {} validate <file.srt|file.vtt|-> [--json] [--strict] [--format srt|vtt]",
        program
    );
    eprintln!(
        "  {} format <file.srt|file.vtt|-> [--json] [--fix] [--format srt|vtt] [--to srt|vtt]",
        program
    );
    eprintln!("  '-' reads the input from stdin; output is always written to stdout.");
}

/// Input format is picked from `--format` if given, otherwise from the file
/// extension: ".vtt" parses as WebVTT, anything else (including no
/// extension, e.g. stdin) parses as SubRip.
fn parse_input(
    path: &str,
    contents: &str,
    format_override: Option<&str>,
) -> (Vec<srt::Cue>, Vec<srt::ParseError>) {
    let is_vtt = match format_override {
        Some(f) => f == "vtt",
        None => match path.rfind('.') {
            Some(idx) => path[idx + 1..].eq_ignore_ascii_case("vtt"),
            None => false,
        },
    };

    if is_vtt {
        vtt::parse(contents)
    } else {
        srt::parse(contents)
    }
}

fn run_validate(
    path: &str,
    contents: &str,
    json_mode: bool,
    strict_mode: bool,
    format_override: Option<&str>,
) {
    let (cues, errors) = parse_input(path, contents, format_override);
    let mut issues = if errors.is_empty() {
        srt::validate(&cues)
    } else {
        Vec::new()
    };
    if errors.is_empty() && strict_mode {
        issues.extend(srt::validate_numbering(&cues));
    }
    let ok = errors.is_empty() && issues.is_empty();

    if json_mode {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"file\": \"{}\",\n", json::escape(path)));
        out.push_str(&format!("  \"valid\": {},\n", ok));
        out.push_str(&format!("  \"cue_count\": {},\n", cues.len()));
        out.push_str("  \"errors\": [\n");
        for (i, e) in errors.iter().enumerate() {
            let comma = if i + 1 < errors.len() { "," } else { "" };
            out.push_str(&format!(
                "    {{ \"line\": {}, \"message\": \"{}\" }}{}\n",
                e.line,
                json::escape(&e.message),
                comma
            ));
        }
        out.push_str("  ],\n");
        out.push_str("  \"issues\": [\n");
        for (i, issue) in issues.iter().enumerate() {
            let comma = if i + 1 < issues.len() { "," } else { "" };
            out.push_str(&format!(
                "    {{ \"cue\": {}, \"message\": \"{}\" }}{}\n",
                issue.cue_number,
                json::escape(&issue.message),
                comma
            ));
        }
        out.push_str("  ]\n");
        out.push_str("}\n");
        print!("{}", out);
    } else if ok {
        println!("{}: valid ({} cues)", path, cues.len());
    } else {
        println!(
            "{}: {} error(s), {} issue(s)",
            path,
            errors.len(),
            issues.len()
        );
        for e in &errors {
            println!("  line {}: {}", e.line, e.message);
        }
        for issue in &issues {
            println!("  cue {}: {}", issue.cue_number, issue.message);
        }
    }

    if !ok {
        process::exit(1);
    }
}

fn run_format(
    path: &str,
    contents: &str,
    json_mode: bool,
    fix_mode: bool,
    format_override: Option<&str>,
    output_format: Option<&str>,
) {
    let (mut cues, errors) = parse_input(path, contents, format_override);
    if !errors.is_empty() {
        eprintln!(
            "{} has {} parse error(s), refusing to format:",
            path,
            errors.len()
        );
        for e in &errors {
            eprintln!("  line {}: {}", e.line, e.message);
        }
        process::exit(1);
    }

    let fixes = if fix_mode { srt::fix(&mut cues) } else { Vec::new() };
    let to_vtt = output_format == Some("vtt");
    let format_tc = |tc: &srt::Timecode| if to_vtt { tc.format_vtt() } else { tc.format() };

    if json_mode {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"file\": \"{}\",\n", json::escape(path)));
        out.push_str("  \"cues\": [\n");
        for (position, cue) in cues.iter().enumerate() {
            let comma = if position + 1 < cues.len() { "," } else { "" };
            out.push_str(&format!(
                "    {{ \"index\": {}, \"start\": \"{}\", \"end\": \"{}\", \"text\": {} }}{}\n",
                position + 1,
                format_tc(&cue.start),
                format_tc(&cue.end),
                json::string_array(&cue.text),
                comma
            ));
        }
        out.push_str("  ],\n");
        out.push_str("  \"fixes\": [\n");
        for (i, f) in fixes.iter().enumerate() {
            let comma = if i + 1 < fixes.len() { "," } else { "" };
            out.push_str(&format!(
                "    {{ \"cue\": {}, \"message\": \"{}\" }}{}\n",
                f.cue_number,
                json::escape(&f.message),
                comma
            ));
        }
        out.push_str("  ]\n");
        out.push_str("}\n");
        print!("{}", out);
    } else {
        if !fixes.is_empty() {
            eprintln!("{}: fixed {} timing issue(s):", path, fixes.len());
            for f in &fixes {
                eprintln!("  cue {}: {}", f.cue_number, f.message);
            }
        }
        if to_vtt {
            print!("{}", vtt::format(&cues));
        } else {
            print!("{}", srt::format(&cues));
        }
    }
}
