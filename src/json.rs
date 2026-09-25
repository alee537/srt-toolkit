// Hand-rolled JSON output. The schemas we emit are small and fixed, so a
// generic value tree plus serializer would be more machinery than the
// problem needs - a couple of string helpers is enough.

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn string_array(items: &[String]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|s| format!("\"{}\"", escape(s)))
        .collect();
    format!("[{}]", parts.join(", "))
}

pub fn opt_string(value: &Option<String>) -> String {
    match value {
        Some(s) => format!("\"{}\"", escape(s)),
        None => "null".to_string(),
    }
}
