//! Structured metadata shared by the native CLI and documentation generator.

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CommandDocumentation {
    pub path: String,
    pub description: String,
    pub usage: String,
    pub arguments: Vec<ArgumentDocumentation>,
    pub examples: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArgumentDocumentation {
    pub name: String,
    pub kind: String,
    pub required: bool,
    pub description: String,
    pub default: Option<String>,
}

/// The CLI's stable, user-facing command contract.
///
/// Runtime dispatch remains in `main.rs`; this metadata is intentionally
/// small and structured so docs do not parse terminal help output.
#[must_use]
pub fn command_documentation() -> Vec<CommandDocumentation> {
    vec![
        command(
            "lattice workspace open",
            "Open and index a canonical workspace.",
            "lattice workspace open <WORKSPACE>",
            vec![path("workspace", "Workspace directory", true)],
            vec!["lattice workspace open ./game".to_owned()],
        ),
        command(
            "lattice workspace status",
            "Inspect canonical workspace identity and index status.",
            "lattice workspace status <WORKSPACE>",
            vec![path("workspace", "Workspace directory", true)],
            vec![],
        ),
        command(
            "lattice index",
            "Incrementally ingest and index a workspace.",
            "lattice index <WORKSPACE>",
            vec![path("workspace", "Workspace directory", true)],
            vec!["lattice index ./game".to_owned()],
        ),
        command(
            "lattice search",
            "Search indexed project names, symbols, paths, and source.",
            "lattice search <WORKSPACE> <QUERY> [--limit <N>]",
            vec![
                path("workspace", "Workspace directory", true),
                string("query", "Search terms", true),
                option("limit", "Maximum results", false, Some("10")),
            ],
            vec!["lattice search ./game inventory --limit 20".to_owned()],
        ),
        command(
            "lattice studio list",
            "List discoverable Studio sessions and platform resolution state.",
            "lattice studio list",
            vec![],
            vec![],
        ),
        command(
            "lattice studio environment",
            "Inspect native Studio environments and path capabilities.",
            "lattice studio environment [--verbose]",
            vec![flag("verbose", "Include detailed resolver diagnostics")],
            vec![],
        ),
        command(
            "lattice studio mcp",
            "Inspect or run the read-only live Studio MCP proof.",
            "lattice studio mcp [--connect]",
            vec![flag("connect", "Launch the resolved MCP child and execute read-only checks")],
            vec![],
        ),
        command(
            "lattice studio pull",
            "Pull Luau source from the connected Studio into a native place.json workspace.",
            "lattice studio pull --output <DIR> [--force] [--dry-run]",
            vec![
                path("output", "Destination workspace", true),
                flag("force", "Replace differing existing files"),
                flag("dry-run", "Compare without writing"),
            ],
            vec!["lattice studio pull --output ./game --dry-run".to_owned()],
        ),
        command(
            "lattice provider list",
            "List registered providers without connecting them.",
            "lattice provider list",
            vec![],
            vec![],
        ),
        command(
            "lattice provider inspect",
            "Inspect one provider descriptor and health model.",
            "lattice provider inspect <PROVIDER>",
            vec![string("provider", "Stable provider ID", true)],
            vec![],
        ),
        command(
            "lattice tool search",
            "Search the unified provider tool catalog.",
            "lattice tool search <QUERY> [--limit <N>]",
            vec![
                string("query", "Tool search terms", true),
                option("limit", "Maximum matches", false, Some("10")),
            ],
            vec!["lattice tool search execute luau".to_owned()],
        ),
        command(
            "lattice tool inspect",
            "Inspect one exact tool reference and its schemas.",
            "lattice tool inspect <TOOL>",
            vec![string("tool", "Canonical lattice:// provider tool reference", true)],
            vec![],
        ),
        command(
            "lattice capability list",
            "List registered semantic capabilities.",
            "lattice capability list",
            vec![],
            vec![],
        ),
        command(
            "lattice mcp stdio",
            "Forward Codex and other MCP clients to the authoritative daemon.",
            "lattice mcp stdio",
            vec![],
            vec!["codex mcp add lattice -- lattice mcp stdio".to_owned()],
        ),
        command(
            "lattice mcp status",
            "Inspect local daemon and northbound MCP availability.",
            "lattice mcp status",
            vec![],
            vec![],
        ),
        command(
            "lattice integration codex install",
            "Configure Codex CLI's global Lattice MCP server entry.",
            "lattice integration codex install",
            vec![],
            vec![],
        ),
        command(
            "lattice integration codex status",
            "Verify Codex installation and daemon reachability.",
            "lattice integration codex status",
            vec![],
            vec![],
        ),
        command(
            "lattice integration codex remove",
            "Remove only the Lattice MCP entry from Codex.",
            "lattice integration codex remove",
            vec![],
            vec![],
        ),
    ]
}

fn command(
    path: &str,
    description: &str,
    usage: &str,
    arguments: Vec<ArgumentDocumentation>,
    examples: Vec<String>,
) -> CommandDocumentation {
    CommandDocumentation {
        path: path.to_owned(),
        description: description.to_owned(),
        usage: usage.to_owned(),
        arguments,
        examples,
    }
}

fn path(name: &str, description: &str, required: bool) -> ArgumentDocumentation {
    ArgumentDocumentation {
        name: name.to_owned(),
        kind: "path".to_owned(),
        required,
        description: description.to_owned(),
        default: None,
    }
}

fn string(name: &str, description: &str, required: bool) -> ArgumentDocumentation {
    ArgumentDocumentation {
        name: name.to_owned(),
        kind: "string".to_owned(),
        required,
        description: description.to_owned(),
        default: None,
    }
}

fn option(
    name: &str,
    description: &str,
    required: bool,
    default: Option<&str>,
) -> ArgumentDocumentation {
    ArgumentDocumentation {
        name: name.to_owned(),
        kind: "option".to_owned(),
        required,
        description: description.to_owned(),
        default: default.map(str::to_owned),
    }
}

fn flag(name: &str, description: &str) -> ArgumentDocumentation {
    ArgumentDocumentation {
        name: format!("--{name}"),
        kind: "flag".to_owned(),
        required: false,
        description: description.to_owned(),
        default: Some("false".to_owned()),
    }
}
