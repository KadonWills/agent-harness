//! Detect denied network attempts from sandbox output.
//!
//! With egress denied, a program that tries the network fails with a
//! resolver or connect error, and nearly every runtime prints one. This
//! module scans step output for those signatures and records destination
//! and originating step. It is deterministic, but it cannot see attempts a
//! program swallows silently — see RFC-0001 §4.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use harness_core::manifest::EgressEvent;
use regex::Regex;

/// Signatures whose first capture group, when present, is the destination.
const SIGNATURES: &[&str] = &[
    // Node.js
    r"getaddrinfo (?:ENOTFOUND|EAI_AGAIN) ([A-Za-z0-9.-]+)",
    r"connect (?:ENETUNREACH|ECONNREFUSED|EHOSTUNREACH) ([0-9A-Fa-f.:]+)",
    // curl, git, libcurl users
    r"Could not resolve host: ([A-Za-z0-9.-]+)",
    r"unable to access '[a-z]+://([^/':]+)",
    // Python (urllib3 / requests / pip)
    r"Failed to resolve '([A-Za-z0-9.-]+)'",
    r"host='([A-Za-z0-9.-]+)', port=\d+\): Max retries exceeded",
    // Go
    r"dial tcp: lookup ([A-Za-z0-9.-]+)",
    // busybox / wget
    r"bad address '([A-Za-z0-9.-]+)'",
    r"unable to resolve host address '([A-Za-z0-9.-]+)'",
    // Generic resolver/socket failures with no destination in the line.
    r"Temporary failure in name resolution",
    r"Name or service not known",
    r"Network is unreachable",
];

fn signatures() -> &'static [Regex] {
    static COMPILED: OnceLock<Vec<Regex>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        SIGNATURES
            .iter()
            .filter_map(|s| Regex::new(s).ok())
            .collect()
    })
}

/// Scan one step's combined output. Each matching line yields one event;
/// a line matching several signatures yields the most specific one.
pub fn detect(step: &str, output: &str) -> Vec<EgressEvent> {
    let mut events = BTreeSet::new();
    for line in output.lines() {
        let line = line.trim();
        let hit = signatures().iter().find_map(|re| {
            re.captures(line).map(|caps| {
                caps.get(1)
                    .map_or("unknown", |m| m.as_str().trim_end_matches('.'))
                    .to_string()
            })
        });
        if let Some(destination) = hit {
            events.insert(EgressEvent {
                step: step.to_string(),
                destination,
                evidence: truncate(line, 300),
            });
        }
    }
    events.into_iter().collect()
}

fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn destinations(output: &str) -> Vec<String> {
        detect("step", output)
            .into_iter()
            .map(|e| e.destination)
            .collect()
    }

    #[test]
    fn recognizes_common_runtimes() {
        let cases = [
            (
                "Error: getaddrinfo ENOTFOUND registry.npmjs.org",
                "registry.npmjs.org",
            ),
            ("curl: (6) Could not resolve host: example.com", "example.com"),
            (
                "fatal: unable to access 'https://github.com/x/y.git/': Could not resolve host: github.com",
                "github.com",
            ),
            (
                "NameResolutionError: Failed to resolve 'pypi.org' ([Errno -3] Temporary failure in name resolution)",
                "pypi.org",
            ),
            (
                "Get \"https://proxy.golang.org/\": dial tcp: lookup proxy.golang.org on 127.0.0.11:53",
                "proxy.golang.org",
            ),
            ("wget: bad address 'evil.example'", "evil.example"),
            ("connect ENETUNREACH 1.2.3.4:443", "1.2.3.4:443"),
            ("socket.gaierror: [Errno -2] Name or service not known", "unknown"),
        ];
        for (line, want) in cases {
            assert_eq!(destinations(line), [want], "line: {line}");
        }
    }

    #[test]
    fn ignores_ordinary_output_and_dedups() {
        assert!(detect("npm test", "PASS src/a.test.ts\n3 tests passed\n").is_empty());
        let twice = "getaddrinfo ENOTFOUND a.com\ngetaddrinfo ENOTFOUND a.com\n";
        let events = detect("npm test", twice);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].step, "npm test");
    }

    #[test]
    fn truncates_long_evidence() {
        let line = format!("Could not resolve host: a.com {}", "x".repeat(1000));
        let events = detect("s", &line);
        assert!(events[0].evidence.chars().count() <= 301);
    }
}
