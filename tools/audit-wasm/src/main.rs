/// audit-wasm — checks release WASM artifacts against an import/export allowlist policy.
///
/// Usage:
///   audit-wasm --policy <policy-file> <wasm-file> [<wasm-file> ...]
///
/// Exit codes:
///   0  all artifacts conform to the policy
///   1  one or more violations found, or a usage / parse error occurred
///
/// The tool parses each artifact with the `wasmparser` streaming parser and
/// never executes the WASM binary.  Every import and export name is extracted,
/// compared against the allowlist, and any symbol that is absent from the
/// policy is reported as a violation.
///
/// Policy file format (plain text, UTF-8):
///   # lines starting with '#' are comments and are ignored
///   blank lines are ignored
///   [imports]      — section header; following entries are import allowlist
///   [exports]      — section header; following entries are export allowlist
///   module::name   — a permitted import (module and name separated by "::")
///   name           — a permitted export (no "::" separator)
///
/// Duplicate allowlist entries are silently de-duplicated.

use std::collections::HashSet;
use std::env;
use std::fmt;
use std::fs;
use std::path::Path;
use std::process;

use wasmparser::{Parser, Payload};

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

/// The parsed allowlist loaded from the policy file.
#[derive(Debug, Default)]
struct Policy {
    allowed_imports: HashSet<String>,
    allowed_exports: HashSet<String>,
}

impl Policy {
    /// Load and parse the policy file.
    fn load(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("cannot read policy file {}: {}", path.display(), e))?;
        Self::parse(&content)
    }

    /// Parse the policy file content.
    fn parse(content: &str) -> Result<Self, String> {
        let mut policy = Policy::default();
        let mut section: Option<Section> = None;

        for (lineno, raw) in content.lines().enumerate() {
            let line = raw.trim();
            // skip blank lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // section headers
            if line == "[imports]" {
                section = Some(Section::Imports);
                continue;
            }
            if line == "[exports]" {
                section = Some(Section::Exports);
                continue;
            }
            match section {
                Some(Section::Imports) => {
                    // expect "module::name"
                    if !line.contains("::") {
                        return Err(format!(
                            "policy line {}: import entry must be 'module::name', got: {}",
                            lineno + 1,
                            line
                        ));
                    }
                    policy.allowed_imports.insert(line.to_string());
                }
                Some(Section::Exports) => {
                    policy.allowed_exports.insert(line.to_string());
                }
                None => {
                    return Err(format!(
                        "policy line {}: entry before any [imports] or [exports] header: {}",
                        lineno + 1,
                        line
                    ));
                }
            }
        }
        Ok(policy)
    }
}

#[derive(Debug)]
enum Section {
    Imports,
    Exports,
}

// ---------------------------------------------------------------------------
// WASM analysis
// ---------------------------------------------------------------------------

/// A single import or export found in a WASM module.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Symbol {
    Import { module: String, name: String },
    Export { name: String },
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Symbol::Import { module, name } => write!(f, "import  {}::{}", module, name),
            Symbol::Export { name } => write!(f, "export  {}", name),
        }
    }
}

/// Extract every import and export from a WASM binary.
///
/// This function never executes the WASM; it only reads section headers and
/// the import/export section payloads via the wasmparser streaming API.
fn extract_symbols(wasm: &[u8], source: &str) -> Result<Vec<Symbol>, String> {
    let mut symbols = Vec::new();

    for payload in Parser::new(0).parse_all(wasm) {
        let payload =
            payload.map_err(|e| format!("{}: wasmparser error: {}", source, e))?;

        match payload {
            Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import
                        .map_err(|e| format!("{}: import section parse error: {}", source, e))?;
                    symbols.push(Symbol::Import {
                        module: import.module.to_string(),
                        name: import.name.to_string(),
                    });
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export
                        .map_err(|e| format!("{}: export section parse error: {}", source, e))?;
                    symbols.push(Symbol::Export {
                        name: export.name.to_string(),
                    });
                }
            }
            // All other sections are intentionally skipped — this tool only
            // inspects the public interface, never the function bodies.
            _ => {}
        }
    }

    Ok(symbols)
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

/// The result of auditing one WASM file.
struct AuditResult {
    path: String,
    violations: Vec<Symbol>,
    allowed_imports_seen: Vec<String>,
    allowed_exports_seen: Vec<String>,
}

/// Audit a single WASM artifact against the policy.
fn audit_file(path: &Path, policy: &Policy) -> Result<AuditResult, String> {
    let wasm = fs::read(path)
        .map_err(|e| format!("cannot read WASM file {}: {}", path.display(), e))?;

    let source = path.display().to_string();
    let symbols = extract_symbols(&wasm, &source)?;

    let mut violations = Vec::new();
    let mut allowed_imports_seen = Vec::new();
    let mut allowed_exports_seen = Vec::new();

    for symbol in &symbols {
        match symbol {
            Symbol::Import { module, name } => {
                let key = format!("{}::{}", module, name);
                if policy.allowed_imports.contains(&key) {
                    allowed_imports_seen.push(key);
                } else {
                    violations.push(symbol.clone());
                }
            }
            Symbol::Export { name } => {
                if policy.allowed_exports.contains(name) {
                    allowed_exports_seen.push(name.clone());
                } else {
                    violations.push(symbol.clone());
                }
            }
        }
    }

    Ok(AuditResult {
        path: path.display().to_string(),
        violations,
        allowed_imports_seen,
        allowed_exports_seen,
    })
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

fn usage() -> ! {
    eprintln!("Usage: audit-wasm --policy <policy-file> <wasm-file> [<wasm-file> ...]");
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse --policy flag and positional WASM file arguments.
    let mut policy_path: Option<String> = None;
    let mut wasm_paths: Vec<String> = Vec::new();
    let mut iter = args.iter().skip(1); // skip argv[0]

    while let Some(arg) = iter.next() {
        if arg == "--policy" {
            policy_path = Some(iter.next().unwrap_or_else(|| {
                eprintln!("error: --policy requires an argument");
                usage();
            }).clone());
        } else if arg.starts_with("--") {
            eprintln!("error: unknown flag: {}", arg);
            usage();
        } else {
            wasm_paths.push(arg.clone());
        }
    }

    let policy_path = policy_path.unwrap_or_else(|| {
        eprintln!("error: --policy is required");
        usage();
    });

    if wasm_paths.is_empty() {
        eprintln!("error: at least one WASM file is required");
        usage();
    }

    // Load policy.
    let policy = Policy::load(Path::new(&policy_path)).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        process::exit(1);
    });

    // Audit each artifact.
    let mut any_violations = false;

    for wasm_path in &wasm_paths {
        let result = audit_file(Path::new(wasm_path), &policy).unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            process::exit(1);
        });

        println!("artifact: {}", result.path);
        println!(
            "  imports allowed : {}",
            result.allowed_imports_seen.len()
        );
        println!(
            "  exports allowed : {}",
            result.allowed_exports_seen.len()
        );

        if result.violations.is_empty() {
            println!("  status          : ok");
        } else {
            println!("  status          : VIOLATIONS FOUND");
            any_violations = true;
            for v in &result.violations {
                println!("  VIOLATION       : {}", v);
            }
        }
        println!();
    }

    if any_violations {
        eprintln!(
            "audit failed: one or more symbols are absent from the policy allowlist in {}",
            policy_path
        );
        process::exit(1);
    }

    println!("audit passed: all symbols conform to the policy in {}", policy_path);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Policy parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parses_empty_policy() {
        let p = Policy::parse("").unwrap();
        assert!(p.allowed_imports.is_empty());
        assert!(p.allowed_exports.is_empty());
    }

    #[test]
    fn parses_comments_and_blank_lines() {
        let src = "
# this is a comment

# another comment
[imports]
# import comment
[exports]
# export comment
";
        let p = Policy::parse(src).unwrap();
        assert!(p.allowed_imports.is_empty());
        assert!(p.allowed_exports.is_empty());
    }

    #[test]
    fn parses_import_entries() {
        let src = "[imports]\nlenv::_\nlenv::require_auth\n";
        let p = Policy::parse(src).unwrap();
        assert!(p.allowed_imports.contains("lenv::_"));
        assert!(p.allowed_imports.contains("lenv::require_auth"));
        assert_eq!(p.allowed_imports.len(), 2);
    }

    #[test]
    fn parses_export_entries() {
        let src = "[exports]\ninitialize\nregister_proof\n";
        let p = Policy::parse(src).unwrap();
        assert!(p.allowed_exports.contains("initialize"));
        assert!(p.allowed_exports.contains("register_proof"));
    }

    #[test]
    fn parses_mixed_sections() {
        let src = "[imports]\na::b\n[exports]\nfoo\n[imports]\nc::d\n";
        let p = Policy::parse(src).unwrap();
        assert!(p.allowed_imports.contains("a::b"));
        assert!(p.allowed_imports.contains("c::d"));
        assert!(p.allowed_exports.contains("foo"));
    }

    #[test]
    fn deduplicates_allowlist_entries() {
        let src = "[imports]\na::b\na::b\n[exports]\nfoo\nfoo\n";
        let p = Policy::parse(src).unwrap();
        assert_eq!(p.allowed_imports.len(), 1);
        assert_eq!(p.allowed_exports.len(), 1);
    }

    #[test]
    fn rejects_import_without_separator() {
        let src = "[imports]\nbad_entry\n";
        assert!(Policy::parse(src).is_err());
    }

    #[test]
    fn rejects_entry_before_section_header() {
        let src = "a::b\n[imports]\n";
        assert!(Policy::parse(src).is_err());
    }

    // -----------------------------------------------------------------------
    // Symbol display
    // -----------------------------------------------------------------------

    #[test]
    fn import_symbol_display() {
        let s = Symbol::Import {
            module: "lenv".into(),
            name: "require_auth".into(),
        };
        assert_eq!(format!("{}", s), "import  lenv::require_auth");
    }

    #[test]
    fn export_symbol_display() {
        let s = Symbol::Export {
            name: "initialize".into(),
        };
        assert_eq!(format!("{}", s), "export  initialize");
    }

    // -----------------------------------------------------------------------
    // WASM parsing using embedded WAT snippets compiled via the `wat` crate.
    // -----------------------------------------------------------------------

    /// Compile a minimal WAT module with one import and one export.
    fn minimal_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
              (import "lenv" "require_auth" (func))
              (func (export "initialize"))
            )
            "#,
        )
        .expect("wat compile failed")
    }

    /// Compile a WAT module with a forbidden extra import.
    fn forbidden_import_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
              (import "lenv" "require_auth" (func))
              (import "env" "forbidden_host_call" (func))
              (func (export "initialize"))
            )
            "#,
        )
        .expect("wat compile failed")
    }

    /// Compile a WAT module with a forbidden extra export.
    fn forbidden_export_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
              (import "lenv" "require_auth" (func))
              (func (export "initialize"))
              (func (export "unexpected_export"))
            )
            "#,
        )
        .expect("wat compile failed")
    }

    /// Compile a WAT module with no imports or exports.
    fn empty_wasm() -> Vec<u8> {
        wat::parse_str("(module)").expect("wat compile failed")
    }

    fn minimal_policy() -> Policy {
        Policy::parse(
            "[imports]\nlenv::require_auth\n[exports]\ninitialize\n",
        )
        .unwrap()
    }

    // -----------------------------------------------------------------------
    // Positive tests: conforming artifacts pass
    // -----------------------------------------------------------------------

    #[test]
    fn conforming_artifact_passes() {
        let wasm = minimal_wasm();
        let policy = minimal_policy();
        let symbols = extract_symbols(&wasm, "test").unwrap();
        let mut violations = vec![];
        for s in &symbols {
            match s {
                Symbol::Import { module, name } => {
                    if !policy.allowed_imports.contains(&format!("{}::{}", module, name)) {
                        violations.push(s.clone());
                    }
                }
                Symbol::Export { name } => {
                    if !policy.allowed_exports.contains(name) {
                        violations.push(s.clone());
                    }
                }
            }
        }
        assert!(violations.is_empty(), "unexpected violations: {:?}", violations);
    }

    #[test]
    fn empty_module_against_empty_policy_passes() {
        let wasm = empty_wasm();
        let policy = Policy::parse("").unwrap();
        let symbols = extract_symbols(&wasm, "empty").unwrap();
        assert!(symbols.is_empty());
        // No symbols means no violations regardless of policy.
        let violations: Vec<_> = symbols
            .iter()
            .filter(|s| match s {
                Symbol::Import { module, name } => {
                    !policy.allowed_imports.contains(&format!("{}::{}", module, name))
                }
                Symbol::Export { name } => !policy.allowed_exports.contains(name),
            })
            .collect();
        assert!(violations.is_empty());
    }

    #[test]
    fn superset_policy_allows_subset_artifact() {
        // Policy allows more than the artifact uses — that is fine.
        let wasm = minimal_wasm();
        let policy = Policy::parse(
            "[imports]\nlenv::require_auth\nlenv::extra_allowed\n[exports]\ninitialize\nextra_export\n",
        )
        .unwrap();
        let symbols = extract_symbols(&wasm, "test").unwrap();
        let violations: Vec<_> = symbols
            .iter()
            .filter(|s| match s {
                Symbol::Import { module, name } => {
                    !policy.allowed_imports.contains(&format!("{}::{}", module, name))
                }
                Symbol::Export { name } => !policy.allowed_exports.contains(name),
            })
            .collect();
        assert!(violations.is_empty());
    }

    // -----------------------------------------------------------------------
    // Negative tests: non-conforming artifacts fail
    // -----------------------------------------------------------------------

    #[test]
    fn forbidden_import_produces_violation() {
        let wasm = forbidden_import_wasm();
        let policy = minimal_policy();
        let symbols = extract_symbols(&wasm, "test").unwrap();
        let violations: Vec<_> = symbols
            .iter()
            .filter(|s| match s {
                Symbol::Import { module, name } => {
                    !policy.allowed_imports.contains(&format!("{}::{}", module, name))
                }
                Symbol::Export { name } => !policy.allowed_exports.contains(name),
            })
            .collect();
        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0],
            &Symbol::Import {
                module: "env".into(),
                name: "forbidden_host_call".into(),
            }
        );
    }

    #[test]
    fn forbidden_export_produces_violation() {
        let wasm = forbidden_export_wasm();
        let policy = minimal_policy();
        let symbols = extract_symbols(&wasm, "test").unwrap();
        let violations: Vec<_> = symbols
            .iter()
            .filter(|s| match s {
                Symbol::Import { module, name } => {
                    !policy.allowed_imports.contains(&format!("{}::{}", module, name))
                }
                Symbol::Export { name } => !policy.allowed_exports.contains(name),
            })
            .collect();
        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0],
            &Symbol::Export {
                name: "unexpected_export".into(),
            }
        );
    }

    #[test]
    fn multiple_violations_all_reported() {
        let wasm = wat::parse_str(
            r#"
            (module
              (import "env" "bad_import_a" (func))
              (import "env" "bad_import_b" (func))
              (func (export "bad_export"))
            )
            "#,
        )
        .unwrap();
        // Empty policy means every symbol is a violation.
        let policy = Policy::parse("").unwrap();
        let symbols = extract_symbols(&wasm, "test").unwrap();
        assert_eq!(symbols.len(), 3);
        let violations: Vec<_> = symbols
            .iter()
            .filter(|s| match s {
                Symbol::Import { module, name } => {
                    !policy.allowed_imports.contains(&format!("{}::{}", module, name))
                }
                Symbol::Export { name } => !policy.allowed_exports.contains(name),
            })
            .collect();
        // All three symbols are violations because the policy allows nothing.
        assert_eq!(violations.len(), 3);
    }

    // -----------------------------------------------------------------------
    // Boundary tests
    // -----------------------------------------------------------------------

    #[test]
    fn extracts_no_symbols_from_empty_module() {
        let wasm = empty_wasm();
        let symbols = extract_symbols(&wasm, "empty").unwrap();
        assert!(symbols.is_empty());
    }

    #[test]
    fn rejects_invalid_wasm_bytes() {
        let bad = b"not a wasm binary";
        let result = extract_symbols(bad, "bad");
        assert!(result.is_err());
    }

    #[test]
    fn policy_with_only_imports_section_has_no_exports() {
        let p = Policy::parse("[imports]\na::b\n").unwrap();
        assert!(p.allowed_exports.is_empty());
    }

    #[test]
    fn policy_with_only_exports_section_has_no_imports() {
        let p = Policy::parse("[exports]\nfoo\n").unwrap();
        assert!(p.allowed_imports.is_empty());
    }

    // -----------------------------------------------------------------------
    // Authorization / privacy regression
    // -----------------------------------------------------------------------

    /// A WASM that imports only the Soroban `lenv` module's symbols cannot
    /// reach outside the Soroban host.  Confirm that no `env` module imports
    /// appear in a minimal Soroban-style contract.
    #[test]
    fn soroban_style_contract_has_no_env_imports() {
        // A minimal WASM that uses only the `lenv` host module (Soroban).
        let wasm = wat::parse_str(
            r#"
            (module
              (import "lenv" "0" (func))
              (import "lenv" "1" (func))
              (func (export "initialize"))
              (func (export "__data_end"))
              (func (export "memory"))
            )
            "#,
        )
        .unwrap();
        let symbols = extract_symbols(&wasm, "test").unwrap();
        for s in &symbols {
            if let Symbol::Import { module, .. } = s {
                assert_ne!(
                    module, "env",
                    "unexpected env import in Soroban contract: {}",
                    s
                );
            }
        }
    }
}
