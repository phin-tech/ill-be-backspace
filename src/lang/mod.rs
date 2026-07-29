//! Language registry and file-type detection.

pub mod spec;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

pub use spec::{BlockComment, DocstringStyle, LanguageSpec, StringSpec};

macro_rules! builtin_languages {
    ($($file:literal),* $(,)?) => {
        &[$(($file, include_str!(concat!("../../languages/", $file)))),*]
    };
}

/// `(file name, contents)` for each shipped language, embedded at compile time.
const BUILTIN: &[(&str, &str)] = builtin_languages![
    "python.toml",
    "rust.toml",
    "go.toml",
    "javascript.toml",
    "typescript.toml",
    "bash.toml",
    "c.toml",
    "java.toml",
    "kotlin.toml",
    "swift.toml",
    "ruby.toml",
    "php.toml",
    "lua.toml",
    "sql.toml",
    "yaml.toml",
    "toml.toml",
    "hcl.toml",
    "dockerfile.toml",
    "makefile.toml",
];

/// A set of language definitions with lookup tables built for fast detection.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    langs: Vec<Arc<LanguageSpec>>,
    by_extension: HashMap<String, Arc<LanguageSpec>>,
    by_filename: HashMap<String, Arc<LanguageSpec>>,
    by_name: HashMap<String, Arc<LanguageSpec>>,
}

impl Registry {
    /// The built-in languages. Parsed once; a malformed built-in spec is a bug, so
    /// this panics rather than making every caller handle an impossible error.
    pub fn builtin() -> &'static Registry {
        static REGISTRY: OnceLock<Registry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let mut reg = Registry::default();
            for (path, src) in BUILTIN {
                let spec: LanguageSpec = toml::from_str(src)
                    .unwrap_or_else(|e| panic!("built-in language `{path}` is invalid: {e}"));
                reg.insert(spec)
                    .unwrap_or_else(|e| panic!("built-in language `{path}` is invalid: {e}"));
            }
            reg
        })
    }

    /// Adds a language, overriding any existing one with the same name. User
    /// languages are inserted after the built-ins, so they win on conflicting
    /// extensions — that is what makes `[[languages.custom]]` able to replace a
    /// shipped definition, not just extend the set.
    pub fn insert(&mut self, spec: LanguageSpec) -> Result<(), String> {
        let spec = Arc::new(spec.normalize()?);
        for ext in &spec.extensions {
            self.by_extension.insert(ext.clone(), Arc::clone(&spec));
        }
        for name in &spec.filenames {
            self.by_filename.insert(name.clone(), Arc::clone(&spec));
        }
        self.by_name
            .insert(spec.name.to_ascii_lowercase(), Arc::clone(&spec));
        self.langs.retain(|l| l.name != spec.name);
        self.langs.push(spec);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<LanguageSpec>> {
        self.by_name.get(&name.to_ascii_lowercase())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<LanguageSpec>> {
        self.langs.iter()
    }

    /// Identifies a file by name, then by shebang. `source` is only consulted when
    /// the name is inconclusive, so the common path never scans the file twice.
    pub fn detect(&self, path: &Path, source: &str) -> Option<&Arc<LanguageSpec>> {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(lang) = self.by_filename.get(name) {
                return Some(lang);
            }
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let key = format!(".{}", ext.to_ascii_lowercase());
            if let Some(lang) = self.by_extension.get(&key) {
                return Some(lang);
            }
        }
        self.detect_by_shebang(source)
    }

    pub fn detect_by_shebang(&self, source: &str) -> Option<&Arc<LanguageSpec>> {
        let first = source.lines().next()?;
        let rest = first.strip_prefix("#!")?;
        // Prefer the longest match so `python3` beats `python` when both are listed
        // by different languages.
        self.langs
            .iter()
            .flat_map(|lang| {
                lang.shebangs
                    .iter()
                    .filter(|s| word_in_command(rest, s))
                    .map(move |s| (s.len(), lang))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, lang)| lang)
    }
}

/// Matches a shebang interpreter as a whole path segment, so `/usr/bin/env python3`
/// matches `python3` but `/opt/pythonic/bin/tool` does not match `python`.
fn word_in_command(command: &str, needle: &str) -> bool {
    command
        .split([' ', '\t', '/'])
        .any(|part| part == needle || part.strip_suffix(".exe") == Some(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_parses() {
        let reg = Registry::builtin();
        assert_eq!(reg.iter().count(), BUILTIN.len());
    }

    #[test]
    fn detects_by_extension_and_filename() {
        let reg = Registry::builtin();
        assert_eq!(reg.detect(Path::new("a/b.rs"), "").unwrap().name, "rust");
        assert_eq!(reg.detect(Path::new("a/B.PY"), "").unwrap().name, "python");
        assert_eq!(
            reg.detect(Path::new("Makefile"), "").unwrap().name,
            "makefile"
        );
        assert!(reg.detect(Path::new("notes.xyz"), "").is_none());
    }

    #[test]
    fn detects_by_shebang_only_for_whole_segments() {
        let reg = Registry::builtin();
        assert_eq!(
            reg.detect(Path::new("script"), "#!/usr/bin/env python3\n")
                .unwrap()
                .name,
            "python"
        );
        assert_eq!(
            reg.detect(Path::new("script"), "#!/bin/bash\n")
                .unwrap()
                .name,
            "bash"
        );
        assert!(reg
            .detect(Path::new("script"), "#!/opt/pythonic/bin/tool\n")
            .is_none());
    }

    #[test]
    fn delimiters_are_sorted_longest_first() {
        let py = Registry::builtin().get("python").unwrap().clone();
        assert_eq!(py.strings[0].delim, "\"\"\"");
    }
}
