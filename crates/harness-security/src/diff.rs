//! Extract added lines from a unified diff (`git diff -U0` output).

/// One line an agent added, with its 1-based line number in the new file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedLine {
    pub path: String,
    pub line: u32,
    pub text: String,
}

fn strip_prefix_path(raw: &str) -> Option<String> {
    let raw = raw.trim_end().trim_matches('"');
    if raw == "/dev/null" {
        return None;
    }
    let path = raw.strip_prefix("b/").unwrap_or(raw);
    Some(path.to_string())
}

/// `@@ -a,b +c,d @@` -> `c`.
fn hunk_start(header: &str) -> Option<u32> {
    let plus = header.split_whitespace().find(|t| t.starts_with('+'))?;
    plus[1..].split(',').next()?.parse().ok()
}

/// Every added line, per file, in diff order. Deleted files and binary
/// changes contribute nothing.
pub fn added_lines(diff: &str) -> Vec<AddedLine> {
    let mut out = Vec::new();
    let mut path: Option<String> = None;
    let mut next_line = 0u32;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            path = strip_prefix_path(rest);
        } else if line.starts_with("diff --git ") {
            path = None;
        } else if line.starts_with("@@") {
            next_line = hunk_start(line).unwrap_or(0);
        } else if let Some(text) = line.strip_prefix('+') {
            if let Some(p) = &path {
                out.push(AddedLine {
                    path: p.clone(),
                    line: next_line,
                    text: text.to_string(),
                });
            }
            next_line += 1;
        } else if line.starts_with(' ') {
            next_line += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_paths_and_new_line_numbers() {
        let diff = "\
diff --git a/src/app.ts b/src/app.ts
index 1..2 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -3,0 +4,2 @@ export function a() {
+const x = 1;
+const y = 2;
@@ -10 +12 @@
-old
+new
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
diff --git a/\"sp ace.txt\" b/\"sp ace.txt\"
--- /dev/null
+++ \"b/sp ace.txt\"
@@ -0,0 +1 @@
+hi
";
        let got: Vec<_> = added_lines(diff)
            .into_iter()
            .map(|l| (l.path, l.line, l.text))
            .collect();
        assert_eq!(
            got,
            [
                ("src/app.ts".into(), 4, "const x = 1;".into()),
                ("src/app.ts".into(), 5, "const y = 2;".into()),
                ("src/app.ts".into(), 12, "new".into()),
                ("sp ace.txt".into(), 1, "hi".into()),
            ]
        );
    }

    #[test]
    fn context_lines_advance_the_counter() {
        let diff = "+++ b/a\n@@ -1,3 +1,4 @@\n ctx\n+added\n ctx\n";
        assert_eq!(added_lines(diff)[0].line, 2);
        assert!(added_lines("").is_empty());
    }
}
