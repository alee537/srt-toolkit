use std::env;
use std::fs;
use std::process;

mod json;
mod srt;

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args.first().map(String::as_str).unwrap_or("srt-toolkit");

    if args.len() < 2 {
        print_usage(program);
        process::exit(2);
    }

    let command = args[1].as_str();
    let mut json_mode = false;
    let mut path: Option<&str> = None;

    for arg in &args[2..] {
        match arg.as_str() {
            "--json" => json_mode = true,
            other if path.is_none() => path = Some(other),
            other => {
                eprintln!("unexpected argument '{}'", other);
                process::exit(2);
            }
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            print_usage(program);
            process::exit(2);
        }
    };

    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("could not read '{}': {}", path, e);
            process::exit(1);
        }
    };

    match command {
        "validate" => run_validate(path, &contents, json_mode),
        "format" => run_format(path, &contents, json_mode),
        other => {
            eprintln!("unknown command '{}'", other);
            print_usage(program);
            process::exit(2);
        }
    }
}

fn print_usage(program: &str) {
    eprintln!("usage:");
    eprintln!("  {} validate <file.srt> [--json]", program);
    eprintln!("  {} format <file.srt> [--json]", program);
}

fn run_validate(path: &str, contents: &str, json_mode: bool) {
    let (cues, errors) = srt::parse(contents);
    let issues = if errors.is_empty() {
        srt::validate(&cues)
    } else {
        Vec::new()
    };
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

fn run_format(path: &str, contents: &str, json_mode: bool) {
    let (cues, errors) = srt::parse(contents);
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
                cue.start.format(),
                cue.end.format(),
                json::string_array(&cue.text),
                comma
            ));
        }
        out.push_str("  ]\n");
        out.push_str("}\n");
        print!("{}", out);
    } else {
        print!("{}", srt::format(&cues));
    }
}
