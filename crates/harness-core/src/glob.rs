//! Minimal glob matching for config patterns (`agent/*`, `@types/*`).
//! `*` matches any run of characters (including `/`); `?` matches one.

/// Whether `text` matches `pattern` in full.
pub fn matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Whether any pattern matches.
pub fn any(patterns: &[String], text: &str) -> bool {
    patterns.iter().any(|p| matches(p, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(matches("agent/*", "agent/claude-1"));
        assert!(matches("agent/*", "agent/a/b"));
        assert!(!matches("agent/*", "main"));
        assert!(matches("*", ""));
        assert!(matches("event-stream", "event-stream"));
        assert!(!matches("event-stream", "event-streams"));
        assert!(matches("@types/*", "@types/node"));
        assert!(matches("l?dash", "lodash"));
        assert!(matches("*-js", "colors-js"));
        assert!(!matches("a*b", "ac"));
        assert!(any(&["x".into(), "agent/*".into()], "agent/1"));
    }
}
