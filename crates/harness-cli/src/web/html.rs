//! Server-rendered pages. Every interpolated value goes through [`esc`];
//! no third-party assets are loaded (RFC-0002 §7).

use std::fmt::Write as _;

use harness_core::config::Problem;
use harness_core::manifest::{AgentManifest, Verdict};

use crate::check::Report;
use crate::conflicts::Analysis;
use crate::history::Entry;
use crate::scan::Findings;

/// HTML-escape text for element content and quoted attributes.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

pub const PAGES: [(&str, &str); 6] = [
    ("/", "Status"),
    ("/run", "Run"),
    ("/history", "History"),
    ("/findings", "Findings"),
    ("/conflicts", "Conflicts"),
    ("/config", "Config"),
];

const STYLE: &str = r#"
:root{--bg:#f7f7f8;--fg:#1d1d22;--muted:#6b6b76;--card:#fff;--line:#e3e3e8;
--ok:#137a3a;--bad:#b42318;--warn:#a15c07;--accent:#3949ab}
@media (prefers-color-scheme:dark){:root{--bg:#141418;--fg:#ececf1;--muted:#9a9aa6;
--card:#1d1d23;--line:#2c2c35;--ok:#4ade80;--bad:#f87171;--warn:#fbbf24;--accent:#8c9eff}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);
font:15px/1.5 system-ui,-apple-system,Segoe UI,sans-serif}
header{display:flex;flex-wrap:wrap;gap:4px 16px;align-items:center;padding:12px 16px;
border-bottom:1px solid var(--line);background:var(--card)}
header b{margin-right:8px}nav a{color:var(--muted);text-decoration:none;padding:4px 8px;border-radius:6px}
nav a.on{color:var(--accent);background:color-mix(in srgb,var(--accent) 12%,transparent)}
main{max-width:1000px;margin:0 auto;padding:16px}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:14px 16px;margin:0 0 14px}
.ok{color:var(--ok)}.bad{color:var(--bad)}.warn{color:var(--warn)}.muted{color:var(--muted)}
.tag{font-weight:700;font-family:ui-monospace,monospace;margin-right:8px}
code,pre,textarea{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:13px}
pre{white-space:pre-wrap;word-break:break-word;margin:6px 0}
table{border-collapse:collapse;width:100%}td,th{text-align:left;padding:6px 8px;border-bottom:1px solid var(--line)}
textarea{width:100%;min-height:360px;background:var(--bg);color:var(--fg);border:1px solid var(--line);
border-radius:8px;padding:10px}
button{background:var(--accent);color:#fff;border:0;border-radius:8px;padding:8px 14px;font:inherit;cursor:pointer}
button:disabled{opacity:.5}input{font:inherit;padding:6px 8px;border:1px solid var(--line);border-radius:8px;
background:var(--bg);color:var(--fg)}
.row{display:flex;gap:8px;align-items:center;flex-wrap:wrap}
"#;

/// Shared client code: authenticated POSTs, polling the run state.
const SCRIPT: &str = r#"
const TOKEN=document.querySelector('meta[name=harness-token]').content;
function esc(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}
async function post(url,body){const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json',
'X-Harness-Token':TOKEN},body:JSON.stringify(body||{})});let j={};try{j=await r.json()}catch(e){}return [r.status,j]}
function renderRun(s){const el=document.getElementById('live');if(!el)return;let h='';
for(const st of s.steps){const d=st.done;let tag='<span class="tag muted">…</span>';
if(d){tag=(d.exit_code===0&&!d.timed_out)?'<span class="tag ok">ok</span>':'<span class="tag bad">'+(d.timed_out?'TIME':'FAIL')+'</span>'}
h+='<div>'+tag+'<code>'+esc(st.phase)+'</code> '+esc(st.command)+(d?' <span class="muted">('+(d.duration_ms/1000).toFixed(1)+'s, exit '+d.exit_code+')</span>':'')+'</div>'}
if(s.output_tail)h+='<pre class="muted">'+esc(s.output_tail)+'</pre>';
if(s.running)h+='<p class="muted">running…</p>';
if(s.error)h+='<pre class="bad">'+esc(s.error)+'</pre>';
if(s.verdict){h+='<p class="tag '+(s.verdict==='Pass'?'ok':'bad')+'">'+(s.verdict==='Pass'?'PASS':'BLOCKED')+'</p><ul>';
for(const r of s.blocking_reasons)h+='<li>'+esc(r)+'</li>';h+='</ul>';
for(const n of s.notes)h+='<div class="muted">note: '+esc(n)+'</div>';
if(s.session_id)h+='<p><a href="/history/'+encodeURIComponent(s.session_id)+'">open manifest</a></p>'}
el.innerHTML=h||'<p class="muted">No run in this session yet.</p>';
const b=document.getElementById('start');if(b)b.disabled=s.running;if(s.running)setTimeout(poll,500)}
async function poll(){const r=await fetch('/api/run');renderRun(await r.json())}
async function startRun(){const agent=document.getElementById('agent').value.trim();
const [st,j]=await post('/api/run',{agent:agent||null});if(st!==202){alert(j.error||st)}poll()}
async function fixConflicts(){const [st,j]=await post('/api/conflicts/fix');if(st!==200){document.getElementById('msg').textContent=j.error||st;return}location.reload()}
async function saveConfig(confirm){const text=document.getElementById('cfg').value;const out=document.getElementById('msg');
const [st,j]=await post('/api/config',{text,confirm_loosen:!!confirm});
if(st===200){out.innerHTML='<p class="ok">Saved.</p>'+(j.loosened.length?'<p class="warn">Loosened: '+j.loosened.map(esc).join('; ')+'</p>':'');return}
if(st===409){out.innerHTML='<div class="card"><p class="warn"><b>This edit loosens the gate:</b></p><ul>'+j.loosened.map(l=>'<li>'+esc(l)+'</li>').join('')+
'</ul><button onclick="saveConfig(true)">Save anyway</button></div>';return}
let h='<p class="bad">Not saved.</p>';if(j.error)h+='<pre class="bad">'+esc(j.error)+'</pre>';
for(const p of (j.problems||[]))h+='<div><span class="tag bad">FAIL</span>'+esc(p.field)+': '+esc(p.message)+'<div class="muted">fix: '+esc(p.fix)+'</div></div>';out.innerHTML=h}
"#;

pub fn layout(active: &str, token: &str, root: &str, body: &str) -> String {
    let mut nav = String::new();
    for (href, title) in PAGES {
        let on = if href == active { " class=\"on\"" } else { "" };
        let _ = write!(nav, "<a href=\"{href}\"{on}>{title}</a>");
    }
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta name=\"harness-token\" content=\"{token}\">\
         <title>harness ui</title><style>{STYLE}</style></head><body>\
         <header><b>harness</b><nav>{nav}</nav><span class=\"muted\">{root}</span></header>\
         <main>{body}</main><script>{SCRIPT}</script></body></html>",
        token = esc(token),
        root = esc(root),
    )
}

fn problems(out: &mut String, problems: &[Problem]) {
    for p in problems {
        let _ = write!(
            out,
            "<div><span class=\"tag bad\">FAIL</span>{}: {}<div class=\"muted\">fix: {}</div></div>",
            esc(&p.field),
            esc(&p.message),
            esc(&p.fix)
        );
    }
}

pub fn status(report: &Report) -> String {
    let mut out = String::from("<h2>Status</h2><div class=\"card\">");
    problems(&mut out, &report.problems);
    for w in &report.warnings {
        let _ = write!(
            out,
            "<div><span class=\"tag warn\">warn</span>{}</div>",
            esc(w)
        );
    }
    if report.ok {
        out.push_str("<div><span class=\"tag ok\">ok</span>harness is ready</div>");
    } else {
        let _ = write!(
            out,
            "<p class=\"bad\">{} problem(s) found</p>",
            report.problems.len()
        );
    }
    out.push_str("</div>");
    out
}

pub fn run() -> String {
    "<h2>Run</h2><div class=\"card\"><p>Runs the gate on the working tree, exactly like \
     <code>harness run</code>.</p><div class=\"row\"><input id=\"agent\" placeholder=\"agent id (optional)\">\
     <button id=\"start\" onclick=\"startRun()\">Start run</button></div></div>\
     <div class=\"card\" id=\"live\"></div><script>addEventListener('load',poll)</script>"
        .to_string()
}

fn verdict_tag(v: Verdict) -> &'static str {
    match v {
        Verdict::Pass => "<span class=\"tag ok\">PASS</span>",
        Verdict::Blocked => "<span class=\"tag bad\">BLOCKED</span>",
        Verdict::Unknown => "<span class=\"tag warn\">UNKNOWN</span>",
    }
}

pub fn history(entries: &[Entry]) -> String {
    let mut out = String::from("<h2>History</h2><div class=\"card\">");
    if entries.is_empty() {
        out.push_str("<p class=\"muted\">No recorded runs yet. Start one from the Run page.</p>");
    } else {
        out.push_str("<table><tr><th>Verdict</th><th>Agent</th><th>Head</th><th>Session</th></tr>");
        for e in entries {
            let m = &e.manifest;
            let _ = write!(
                out,
                "<tr><td>{}</td><td>{}</td><td><code>{}</code></td>\
                 <td><a href=\"/history/{id}\">{id}</a></td></tr>",
                verdict_tag(m.verdict),
                esc(&m.target_agent),
                esc(m.head.get(..7).unwrap_or("")),
                id = esc(&m.session_id),
            );
        }
        out.push_str("</table>");
    }
    out.push_str("</div>");
    out
}

pub fn manifest(m: &AgentManifest) -> String {
    let mut out = format!(
        "<h2>{} {}</h2><div class=\"card\"><div>agent <b>{}</b> · base <code>{}</code> · head \
         <code>{}</code></div><div class=\"muted\">{}</div>",
        verdict_tag(m.verdict),
        esc(&m.session_id),
        esc(&m.target_agent),
        esc(&m.base_ref),
        esc(m.head.get(..12).unwrap_or("(none)")),
        if m.intent_hash.is_empty() {
            "no intent recorded".to_string()
        } else {
            format!("intent sha256 {}", esc(&m.intent_hash))
        }
    );
    if !m.blocking_reasons.is_empty() {
        out.push_str("<ul>");
        for r in &m.blocking_reasons {
            let _ = write!(out, "<li>{}</li>", esc(r));
        }
        out.push_str("</ul>");
    }
    out.push_str("</div>");
    if !m.steps.is_empty() {
        out.push_str("<h3>Sandbox steps</h3><div class=\"card\">");
        for s in &m.steps {
            let tag = if s.exit_code == 0 && !s.timed_out {
                "<span class=\"tag ok\">ok</span>"
            } else {
                "<span class=\"tag bad\">FAIL</span>"
            };
            let _ = write!(
                out,
                "<div>{tag}<code>{:?}</code> {} <span class=\"muted\">(exit {}, {:.1}s)</span></div>",
                s.phase,
                esc(&s.command),
                s.exit_code,
                s.duration_ms as f64 / 1000.0
            );
        }
        out.push_str("</div>");
    }
    if !m.impacted_files.is_empty() {
        let _ = write!(
            out,
            "<h3>Impacted files ({})</h3><div class=\"card\">",
            m.impacted_files.len()
        );
        for f in &m.impacted_files {
            let _ = write!(out, "<div><code>{}</code>", esc(&f.path));
            for c in &f.ast_changes {
                let _ = write!(out, "<div class=\"muted\">&nbsp;&nbsp;{}</div>", esc(c));
            }
            out.push_str("</div>");
        }
        out.push_str("</div>");
    }
    let audit = &m.security_audit;
    let findings = Findings {
        secrets: audit.secret_findings.clone(),
        dependencies: audit.dependency_findings.clone(),
    };
    if !findings.secrets.is_empty() || !findings.dependencies.is_empty() {
        out.push_str("<h3>Security findings</h3>");
        out.push_str(&finding_rows(&findings));
    }
    if !audit.egress_events.is_empty() {
        out.push_str("<h3>Denied network attempts</h3><div class=\"card\">");
        for e in &audit.egress_events {
            let _ = write!(
                out,
                "<div><span class=\"tag bad\">NET</span>{} during <code>{}</code><pre class=\"muted\">{}</pre></div>",
                esc(&e.destination),
                esc(&e.step),
                esc(&e.evidence)
            );
        }
        out.push_str("</div>");
    }
    if !m.conflicts.is_empty() {
        out.push_str("<h3>Conflicts</h3>");
        out.push_str(&conflict_rows(&m.conflicts));
    }
    out
}

fn finding_rows(f: &Findings) -> String {
    let mut out = String::from("<div class=\"card\">");
    for s in &f.secrets {
        let _ = write!(
            out,
            "<div><span class=\"tag bad\">SECRET</span><code>{}:{}</code> {} <span class=\"muted\">({})</span></div>",
            esc(&s.path),
            s.line,
            esc(&s.rule_id),
            esc(&s.description)
        );
    }
    for d in &f.dependencies {
        let _ = write!(
            out,
            "<div><span class=\"tag bad\">DEP</span>{} <code>{}</code> in <code>{}</code>: {}</div>",
            esc(&d.ecosystem),
            esc(&d.name),
            esc(&d.file),
            esc(&d.reason)
        );
    }
    if f.secrets.is_empty() && f.dependencies.is_empty() {
        out.push_str(
            "<div><span class=\"tag ok\">ok</span>no secrets or policy-violating dependencies</div>",
        );
    }
    out.push_str("</div>");
    out
}

pub fn findings(f: &Findings) -> String {
    format!(
        "<h2>Findings</h2><p class=\"muted\">Secrets in added lines and new dependencies, \
         working tree vs. base (same as <code>harness scan</code>).</p>{}",
        finding_rows(f)
    )
}

fn conflict_rows(conflicts: &[harness_core::manifest::ConflictReport]) -> String {
    let mut out = String::from("<div class=\"card\">");
    for c in conflicts {
        let cls = if c.level >= 2 { "bad" } else { "warn" };
        let _ = write!(
            out,
            "<div><span class=\"tag {cls}\">L{}</span>{} <code>{}</code> \
             <span class=\"muted\">[{} → {}]</span><div>{}</div>",
            c.level,
            esc(&c.kind),
            esc(&c.symbol),
            esc(&c.branches[0]),
            esc(&c.branches[1]),
            esc(&c.explanation)
        );
        if let Some(fix) = &c.fix {
            let _ = write!(out, "<div class=\"muted\">fix: {}</div>", esc(fix));
        }
        out.push_str("</div>");
    }
    if conflicts.is_empty() {
        out.push_str("<div><span class=\"tag ok\">ok</span>no semantic conflicts</div>");
    }
    out.push_str("</div>");
    out
}

pub fn conflicts(a: &Analysis) -> String {
    let mut out = format!(
        "<h2>Conflicts</h2><p>Branches: {}</p>",
        a.branches
            .iter()
            .map(|b| format!("<code>{}</code>", esc(b)))
            .collect::<Vec<_>>()
            .join(" · ")
    );
    out.push_str(&conflict_rows(&a.conflicts));
    if a.conflicts.iter().any(|c| c.level == 1) {
        out.push_str(
            "<div class=\"row\"><button onclick=\"fixConflicts()\">Apply Level 1 fixes to the \
             checked-out branch</button><span id=\"msg\" class=\"bad\"></span></div>",
        );
    }
    for n in &a.notes {
        let _ = write!(out, "<div class=\"muted\">note: {}</div>", esc(n));
    }
    out
}

pub fn config(path: &str, text: &str, error: Option<&str>, found: &[Problem]) -> String {
    let mut out = format!(
        "<h2>Config</h2><p class=\"muted\"><code>{}</code> — saved only when it parses and \
         validates; edits that loosen the gate need confirmation.</p><div class=\"card\">",
        esc(path)
    );
    if let Some(e) = error {
        let _ = write!(out, "<pre class=\"bad\">{}</pre>", esc(e));
    }
    problems(&mut out, found);
    if error.is_none() && found.is_empty() {
        out.push_str("<div><span class=\"tag ok\">ok</span>valid</div>");
    }
    let _ = write!(
        out,
        "</div><textarea id=\"cfg\" spellcheck=\"false\">{}</textarea>\
         <div class=\"row\"><button onclick=\"saveConfig(false)\">Validate &amp; save</button></div>\
         <div id=\"msg\"></div>",
        esc(text)
    );
    out
}

pub fn error(title: &str, message: &str) -> String {
    format!(
        "<h2>{}</h2><div class=\"card\"><pre class=\"bad\">{}</pre></div>",
        esc(title),
        esc(message)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_covers_markup_and_attributes() {
        assert_eq!(
            esc("<script>\"x\" & 'y'</script>"),
            "&lt;script&gt;&quot;x&quot; &amp; &#39;y&#39;&lt;/script&gt;"
        );
    }

    #[test]
    fn manifest_page_escapes_agent_controlled_text() {
        let mut m = AgentManifest::new("s1", "<img src=x onerror=alert(1)>");
        m.block("`<b>` failed");
        m.finalize();
        let page = manifest(&m);
        assert!(!page.contains("<img"), "{page}");
        assert!(page.contains("&lt;img"));
        assert!(page.contains("BLOCKED"));
    }

    #[test]
    fn layout_marks_the_active_page_and_carries_the_token() {
        let page = layout("/run", "t0k", "/repo", "<p>x</p>");
        assert!(page.contains("<a href=\"/run\" class=\"on\">Run</a>"));
        assert!(page.contains("content=\"t0k\""));
        assert!(!page.contains("http"), "no external assets");
    }
}
