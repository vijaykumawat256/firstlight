//! Low-level iCalendar line parsing: unfolding, property/parameter splitting, VEVENT grouping.

#[derive(Clone, Debug)]
pub struct Prop {
    /// Upper-cased property name.
    pub name: String,
    pub params: Vec<(String, String)>,
    /// Raw value (not unescaped; callers decide).
    pub value: String,
}

impl Prop {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

/// Joins folded lines (continuations start with a space or tab).
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if let (Some(rest), Some(last)) = (line.strip_prefix([' ', '\t']), out.last_mut()) {
            last.push_str(rest);
            continue;
        }
        out.push(line.to_string());
    }
    out
}

/// Splits on `sep` ignoring separators inside double quotes.
fn split_unquoted(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut quoted) = (0, false);
    for (i, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == sep && !quoted {
            parts.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&s[start..]);
    parts
}

fn parse_line(line: &str) -> Option<Prop> {
    let mut quoted = false;
    let colon = line.char_indices().find_map(|(i, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        (c == ':' && !quoted).then_some(i)
    })?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = split_unquoted(head, ';').into_iter();
    let name = parts.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = parts
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((
                k.trim().to_ascii_uppercase(),
                v.trim().trim_matches('"').to_string(),
            ))
        })
        .collect();
    Some(Prop {
        name,
        params,
        value: value.to_string(),
    })
}

/// Returns the properties of every top-level VEVENT (nested components such as VALARM are
/// dropped) and whether a VCALENDAR was seen at all.
pub fn parse_lines(text: &str) -> (Vec<Vec<Prop>>, bool) {
    let mut events = Vec::new();
    let mut current: Option<Vec<Prop>> = None;
    let mut nested = 0usize;
    let mut saw_calendar = false;
    for line in unfold(text.trim_start_matches('\u{feff}')) {
        let Some(p) = parse_line(&line) else { continue };
        match (p.name.as_str(), p.value.to_ascii_uppercase().as_str()) {
            ("BEGIN", "VCALENDAR") => saw_calendar = true,
            ("BEGIN", "VEVENT") if current.is_none() => current = Some(Vec::new()),
            ("END", "VEVENT") if nested == 0 => {
                if let Some(e) = current.take() {
                    events.push(e);
                }
            }
            ("BEGIN", _) if current.is_some() => nested += 1,
            ("END", _) if current.is_some() && nested > 0 => nested -= 1,
            _ => {
                if let (0, Some(c)) = (nested, current.as_mut()) {
                    c.push(p);
                }
            }
        }
    }
    (events, saw_calendar)
}
