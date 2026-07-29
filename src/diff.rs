//! Restricting checks to changed lines.
//!
//! Adopting a linter on a mature repo should not surface hundreds of legacy
//! findings, so by default a comment block is only reported when the diff
//! actually touched it.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

pub type ChangedLines = HashMap<PathBuf, BTreeSet<u32>>;

/// Which revision to compare against.
#[derive(Debug, Clone)]
pub enum DiffSpec {
    /// Everything not yet committed, staged or not.
    Working,
    /// The merge base with the given ref, for "what this branch changed".
    MergeBase(String),
}

/// Shelling out to `git` rather than linking a git library: git is present in
/// every context this runs in, and it avoids a large, slow-compiling dependency.
pub fn changed_lines(spec: &DiffSpec, repo_root: &Path) -> Result<ChangedLines> {
    let mut args = vec![
        "diff".to_string(),
        "--unified=0".to_string(),
        "--no-color".to_string(),
    ];
    match spec {
        DiffSpec::Working => args.push("HEAD".to_string()),
        DiffSpec::MergeBase(base) => {
            let merge_base = run_git(repo_root, &["merge-base", base, "HEAD"])
                .with_context(|| format!("could not find a merge base with `{base}`"))?;
            args.push(merge_base.trim().to_string());
        }
    }
    let out = run_git(
        repo_root,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    // Canonicalised so lookups match the walked paths on platforms where the
    // repo root is reached through a symlink (macOS /var -> /private/var).
    Ok(parse(&out, repo_root)
        .into_iter()
        .map(|(p, lines)| (p.canonicalize().unwrap_or(p), lines))
        .collect())
}

pub fn is_git_repo(dir: &Path) -> bool {
    run_git(dir, &["rev-parse", "--git-dir"]).is_ok()
}

pub fn repo_root(dir: &Path) -> Result<PathBuf> {
    let out = run_git(dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim()))
}

fn run_git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .context("failed to run `git` — is it installed and on PATH?")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parses unified diff output into the set of lines that exist in the new file.
/// Pure so it can be tested without a repo.
pub fn parse(diff: &str, root: &Path) -> ChangedLines {
    let mut map: ChangedLines = HashMap::new();
    let mut current: Option<PathBuf> = None;

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            current = match rest.trim() {
                // A deleted file has no new side to check.
                "/dev/null" => None,
                p => Some(root.join(p.strip_prefix("b/").unwrap_or(p))),
            };
            continue;
        }
        let Some(rest) = line.strip_prefix("@@ ") else {
            continue;
        };
        let Some(path) = &current else { continue };
        let Some(plus) = rest.split_whitespace().find(|t| t.starts_with('+')) else {
            continue;
        };
        let spec = &plus[1..];
        let (start, count) = match spec.split_once(',') {
            Some((s, c)) => (s.parse::<u32>().ok(), c.parse::<u32>().ok()),
            None => (spec.parse::<u32>().ok(), Some(1)),
        };
        let (Some(start), Some(count)) = (start, count) else {
            continue;
        };
        // A hunk with a zero-length new side is a pure deletion.
        if count == 0 {
            continue;
        }
        map.entry(path.clone())
            .or_default()
            .extend(start..start + count);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "\
diff --git a/src/a.py b/src/a.py
index 1234567..89abcde 100644
--- a/src/a.py
+++ b/src/a.py
@@ -3,0 +4,2 @@ def f():
+    # one
+    # two
@@ -10 +12 @@ def g():
+    pass
";

    fn parsed() -> ChangedLines {
        parse(DIFF, Path::new(""))
    }

    #[test]
    fn collects_added_line_numbers() {
        let m = parsed();
        let lines = m.get(Path::new("src/a.py")).unwrap();
        assert!(lines.contains(&4) && lines.contains(&5));
    }

    #[test]
    fn handles_a_single_line_hunk_without_a_count() {
        assert!(parsed().get(Path::new("src/a.py")).unwrap().contains(&12));
    }

    #[test]
    fn does_not_invent_lines_outside_the_hunk() {
        let m = parsed();
        let lines = m.get(Path::new("src/a.py")).unwrap();
        assert!(!lines.contains(&3));
        assert!(!lines.contains(&6));
    }

    #[test]
    fn ignores_pure_deletions() {
        let d = "+++ b/x.py\n@@ -4,2 +3,0 @@\n-gone\n";
        assert!(!parse(d, Path::new("")).contains_key(Path::new("x.py")));
    }

    #[test]
    fn ignores_deleted_files() {
        let d = "--- a/x.py\n+++ /dev/null\n@@ -1,2 +0,0 @@\n";
        assert!(parse(d, Path::new("")).is_empty());
    }

    #[test]
    fn strips_the_b_prefix_and_joins_the_root() {
        let m = parse("+++ b/src/a.py\n@@ -0,0 +1 @@\n", Path::new("/repo"));
        assert!(m.contains_key(Path::new("/repo/src/a.py")));
    }

    #[test]
    fn handles_several_files() {
        let d = "+++ b/a.py\n@@ -0,0 +1 @@\n+++ b/b.py\n@@ -0,0 +5 @@\n";
        let m = parse(d, Path::new(""));
        assert_eq!(m.len(), 2);
        assert!(m.get(Path::new("b.py")).unwrap().contains(&5));
    }
}
