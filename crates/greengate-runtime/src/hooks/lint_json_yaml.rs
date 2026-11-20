//! pre-commit-lint-json-yaml — parse staged JSON/YAML so a syntax error never
//! reaches the repo. Both linters soft-fail when absent: warn, don't block.
//! JSONC files (tsconfig, `.vscode/`, see [`is_jsonc`]) are parsed in-process
//! with comments and trailing commas allowed.

use super::common::{fail, hl, ok, repo_root, staged_files, warn, which};
use crate::check::Outcome;
use std::path::Path;
use std::process::{Command, Stdio};

/// The extensions each half consumes, and their union.
///
/// EXPORTED so `registry.rs` can DECLARE the scope from the same constant the
/// check FILTERS with. They drifted: the registry declared
/// `[".json", ".yaml", ".yml"]` while this module asked for `[".yaml"]`, so
/// `amont list` reported the check as covering `.yml` and a staged, broken
/// `x.yml` returned `Outcome::Passed` with no output at all. A `const fn` over
/// a `const` is legal in a const item, so the registry can reference these
/// directly and the two cannot disagree again.
pub const JSON: &[&str] = &[".json"];
pub const YAML: &[&str] = &[".yaml", ".yml"];
pub const EXTS: &[&str] = &[".json", ".yaml", ".yml"];

/// Helm chart templates carry Go templating (`{{ }}`) and are not valid YAML
/// until Helm renders them. A staged YAML under a chart's `templates/` — i.e.
/// with a sibling `Chart.yaml` at the chart root — is skipped, or valid chart
/// commits would need --no-verify just to get past this hook.
pub fn is_helm_template(root: &str, file: &str) -> bool {
    let Some(i) = file.find("/templates/") else {
        return false;
    };
    Path::new(root)
        .join(&file[..i])
        .join("Chart.yaml")
        .is_file()
}

/// Files whose own tool reads them as JSONC — JSON with `//` and `/* */`
/// comments and trailing commas: TypeScript's `tsconfig*.json` /
/// `jsconfig*.json`, everything under a `.vscode/` directory, and
/// `devcontainer.json`. Parsing them as strict JSON blocked every commit that
/// touched a commented tsconfig, and pushed repos to move the comments out.
pub fn is_jsonc(file: &str) -> bool {
    let path = Path::new(file);
    let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    (base.starts_with("tsconfig") || base.starts_with("jsconfig") || base == "devcontainer.json")
        && base.ends_with(".json")
        || path.components().any(|c| c.as_os_str() == ".vscode")
}

/// `text` as strict JSON: comments become spaces (newlines kept), a comma
/// whose next token is `}` or `]` is dropped, and a leading BOM goes. String
/// contents are never touched — `"@/*"` in tsconfig `paths` is not a
/// comment. `None` for an unterminated block comment.
pub fn strip_jsonc(text: &str) -> Option<String> {
    let s = text.strip_prefix('\u{feff}').unwrap_or(text).as_bytes();
    // Pass 1: comments out, strings verbatim.
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b'"' => {
                out.push(b'"');
                i += 1;
                while i < s.len() {
                    let c = s[i];
                    out.push(c);
                    i += 1;
                    if c == b'\\' && i < s.len() {
                        out.push(s[i]);
                        i += 1;
                    } else if c == b'"' {
                        break;
                    }
                }
            }
            b'/' if s.get(i + 1) == Some(&b'/') => {
                while i < s.len() && s[i] != b'\n' {
                    out.push(b' ');
                    i += 1;
                }
            }
            b'/' if s.get(i + 1) == Some(&b'*') => {
                let end = s[i + 2..].windows(2).position(|w| w == b"*/")? + i + 4;
                for &c in &s[i..end] {
                    out.push(if c == b'\n' { b'\n' } else { b' ' });
                }
                i = end;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    // Pass 2: trailing commas out, strings verbatim.
    let mut res: Vec<u8> = Vec::with_capacity(out.len());
    let mut i = 0;
    while i < out.len() {
        match out[i] {
            b'"' => {
                res.push(b'"');
                i += 1;
                while i < out.len() {
                    let c = out[i];
                    res.push(c);
                    i += 1;
                    if c == b'\\' && i < out.len() {
                        res.push(out[i]);
                        i += 1;
                    } else if c == b'"' {
                        break;
                    }
                }
            }
            b',' => {
                let next = out[i + 1..].iter().find(|c| !c.is_ascii_whitespace());
                // Only a comma after a value is trailing: `[,]` stays broken.
                let prev = res.iter().rev().find(|c| !c.is_ascii_whitespace());
                if !matches!(next, Some(b'}' | b']')) || matches!(prev, None | Some(b'[' | b'{')) {
                    res.push(b',');
                }
                i += 1;
            }
            c => {
                res.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(res).ok()
}

/// Whether the JSONC file `f` (under `root`) parses. A read error fails, as
/// node's would.
fn parses_jsonc(root: &str, f: &str) -> bool {
    std::fs::read_to_string(Path::new(root).join(f))
        .ok()
        .and_then(|t| strip_jsonc(&t))
        .and_then(|t| crate::json_read::parse(&t))
        .is_some()
}

fn parses(settings: &crate::config::Settings, root: &str, tool: &str, args: &[&str]) -> bool {
    let mut cmd = Command::new(super::common::program(tool));
    cmd.args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    super::common::bounded_success(settings, &mut cmd, tool)
}

pub fn run(settings: &crate::config::Settings, _args: &[std::ffi::OsString]) -> Outcome {
    let (jsonc, json): (Vec<String>, Vec<String>) =
        staged_files(JSON).into_iter().partition(|f| is_jsonc(f));
    let yaml: Vec<String> = staged_files(YAML);
    if json.is_empty() && jsonc.is_empty() && yaml.is_empty() {
        return Outcome::Passed;
    }
    let root = repo_root();
    // Independent halves. A missing parser silences one of them, and a
    // half that never ran must not be reported as a half that passed — so
    // both a failure anywhere and a gap anywhere outrank the clean case.
    let mut failed = false;
    let mut unavailable = false;

    // JSONC is parsed in-process: no node needed, so never a gap.
    for f in &jsonc {
        if !parses_jsonc(&root, f) {
            fail(&format!("Invalid JSONC: {}", hl(f)));
            failed = true;
        }
    }

    if !json.is_empty() {
        if which("node").is_some() {
            for f in &json {
                let script = r#"JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"))"#;
                // `--` before `f`: a staged file named e.g. `-e.json` would
                // otherwise be read as another `-e` by node's own parser.
                if !parses(settings, &root, "node", &["-e", script, "--", f]) {
                    fail(&format!("Invalid JSON: {}", hl(f)));
                    failed = true;
                }
            }
        } else {
            warn(&format!(
                "JSON files detected. To lint them, install {}",
                hl("node")
            ));
            unavailable = true;
        }
    }

    if !yaml.is_empty() {
        if which("yq").is_some() {
            for f in &yaml {
                if is_helm_template(&root, f) {
                    continue;
                }
                // `--` before `f`: a staged file starting with `-` would
                // otherwise be read as a flag by yq's own parser.
                if !parses(settings, &root, "yq", &["e", "true", "--", f]) {
                    fail(&format!("Invalid YAML: {}", hl(f)));
                    failed = true;
                }
            }
        } else {
            warn(&format!(
                "YAML files detected. To lint them, install {}",
                hl("yq")
            ));
            unavailable = true;
        }
    }

    match (failed, unavailable) {
        (true, _) => Outcome::Failed,
        (false, true) => Outcome::Unavailable,
        (false, false) => {
            ok(settings, "Json/Yaml Lint passed");
            Outcome::Passed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_read::parse;

    fn ok(t: &str) -> bool {
        strip_jsonc(t).and_then(|s| parse(&s)).is_some()
    }

    #[test]
    fn jsonc_names() {
        for f in [
            "tsconfig.json",
            "tsconfig.base.json",
            "apps/web/tsconfig.app.json",
            "jsconfig.json",
            ".vscode/settings.json",
            "pkg/.vscode/launch.json",
            ".devcontainer/devcontainer.json",
        ] {
            assert!(is_jsonc(f), "{f}");
        }
        for f in [
            "package.json",
            "my-tsconfig.json",
            "vscode/settings.json",
            "tsconfig.json.bak",
        ] {
            assert!(!is_jsonc(f), "{f}");
        }
    }

    #[test]
    fn a_commented_tsconfig_with_trailing_commas_parses() {
        let t = "\u{feff}{\n  // line\n  /* block\n  */ \"compilerOptions\": {\n    \"paths\": { \"@/*\": [\"./src/*\",], },\n    \"x\": \"http://a//b\", // after\n    \"q\": \"a\\\"//not\",\n    \"t\": true, /* c */\n  },\n}\n";
        assert!(ok(t));
        // The string contents survive untouched.
        let s = strip_jsonc(t).unwrap();
        assert!(
            s.contains("\"@/*\"") && s.contains("\"http://a//b\"") && s.contains("\"a\\\"//not\"")
        );
    }

    #[test]
    fn broken_jsonc_still_fails() {
        assert!(!ok("{ \"a\": 1 /* never closed"));
        assert!(!ok("{ \"a\": }"));
        assert!(!ok("{ \"a\": 1,, }"));
        assert!(!ok("{ \"a\": 1 } trailing"));
        assert!(!ok("[,]"));
    }
}
