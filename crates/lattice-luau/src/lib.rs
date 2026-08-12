//! Safe Lattice-owned semantic facts derived from the official Luau AST.

use lattice_model::{SourcePosition, SourceSpan};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Local,
    Function,
    TypeAlias,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolFact {
    pub name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Global,
    Member,
    Call,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReferenceFact {
    pub name: String,
    pub kind: ReferenceKind,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequireFact {
    pub specifier: String,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    pub message: String,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LuauAnalysis {
    pub symbols: Vec<SymbolFact>,
    pub references: Vec<ReferenceFact>,
    pub requires: Vec<RequireFact>,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub line_count: u64,
}

#[must_use]
pub fn analyze(source: &str) -> LuauAnalysis {
    let native = lattice_luau_sys::analyze(source);
    LuauAnalysis {
        symbols: native
            .symbols
            .into_iter()
            .map(|fact| SymbolFact {
                name: fact.name,
                kind: match fact.kind {
                    1 => SymbolKind::Function,
                    2 => SymbolKind::TypeAlias,
                    _ => SymbolKind::Local,
                },
                span: convert_span(fact.span),
            })
            .collect(),
        references: native
            .references
            .into_iter()
            .map(|fact| ReferenceFact {
                name: fact.name,
                kind: match fact.kind {
                    1 => ReferenceKind::Member,
                    2 => ReferenceKind::Call,
                    _ => ReferenceKind::Global,
                },
                span: convert_span(fact.span),
            })
            .collect(),
        requires: native
            .require_facts
            .into_iter()
            .map(|fact| RequireFact { specifier: fact.specifier, span: convert_span(fact.span) })
            .collect(),
        diagnostics: native
            .diagnostics
            .into_iter()
            .map(|diagnostic| ParseDiagnostic {
                message: diagnostic.message,
                span: convert_span(diagnostic.span),
            })
            .collect(),
        line_count: native.line_count,
    }
}

const fn convert_span(span: lattice_luau_sys::NativeSpan) -> SourceSpan {
    SourceSpan {
        begin: SourcePosition { line: span.begin_line, column: span.begin_column },
        end: SourcePosition { line: span.end_line, column: span.end_column },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_parser_extracts_symbols_and_requires() {
        let source = r"
            export type InventoryItem = { id: string }
            local Inventory = require(script.Parent.Inventory)
            local function grant(player, item)
                Inventory.add(player, item)
            end
        ";
        let analysis = analyze(source);
        assert!(analysis.diagnostics.is_empty(), "{:?}", analysis.diagnostics);
        assert!(analysis.symbols.iter().any(|symbol| symbol.name == "InventoryItem"));
        assert!(analysis.symbols.iter().any(|symbol| symbol.name == "grant"));
        assert_eq!(analysis.requires[0].specifier, "script.Parent.Inventory");
    }

    #[test]
    fn syntax_errors_are_structured() {
        let analysis = analyze("local function broken(");
        assert!(!analysis.diagnostics.is_empty());
    }

    #[test]
    fn lattice_companion_plugin_parses_with_official_luau() -> Result<(), Box<dyn std::error::Error>>
    {
        fn collect_luau_files(
            root: &std::path::Path,
            files: &mut Vec<std::path::PathBuf>,
        ) -> std::io::Result<()> {
            for entry in std::fs::read_dir(root)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    collect_luau_files(&path, files)?;
                } else if path.extension().is_some_and(|extension| extension == "luau") {
                    files.push(path);
                }
            }
            Ok(())
        }

        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/LatticeCompanion");
        let mut files = Vec::new();
        collect_luau_files(&root, &mut files)?;
        files.sort();
        assert!(!files.is_empty(), "plugin fixture must contain Luau modules");
        for path in files {
            let source = std::fs::read_to_string(&path)?;
            let analysis = analyze(&source);
            assert!(
                analysis.diagnostics.is_empty(),
                "{}: {:?}",
                path.display(),
                analysis.diagnostics
            );
        }
        Ok(())
    }
}
