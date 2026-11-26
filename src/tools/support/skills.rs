//! Agent Skills served over MCP (SEP-2640).
//!
//! The skills in `longbridge/skills` are written for the `longbridge` CLI:
//! they tell the model to run shell commands, install the CLI, pass `--yes`,
//! and, for DCA plans, to collect a confirmation itself because the CLI has no
//! dry run. Served over MCP none of that holds: there is no shell, and every
//! write is gated by this server (see `support::confirm`). So each `SKILL.md`
//! is rewritten when it is loaded:
//!
//! - an MCP preamble right after the frontmatter tells the model how to read
//!   the CLI instructions here, and that the tool's own preview is the
//!   confirmation step;
//! - the `Triggers:` keyword list is cut from `description`. It is how the CLI
//!   agent decides to load a skill; over MCP it only spends the client's
//!   context on every connection.
//!
//! The rewritten copy is written to a temporary directory and served from
//! there with rmcp's [`SkillCatalog`]. `references/` and every other file are
//! served unchanged.
//!
//! Skills marked `risk_level: mutating` (orders, DCA, watchlist writes) are
//! hidden on the read-only `/v2` endpoint, whose tools cannot act on them.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use rmcp::ErrorData as McpError;
use rmcp::handler::server::skill_catalog::SkillCatalog;
use rmcp::model::{
    GetSkillResult, ListResourcesResult, ListSkillsResult, ReadResourceResult, Resource, Skill,
};

/// Directory holding the skills to serve, one folder per skill.
const SOURCE_ENV: &str = "LONGBRIDGE_MCP_SKILLS_DIR";
/// Optional: re-read the skills every this many seconds, for a directory
/// mounted from outside the image (e.g. a ConfigMap) that changes without a
/// redeploy.
const RELOAD_ENV: &str = "LONGBRIDGE_MCP_SKILLS_RELOAD_SECS";

const URI_SCHEME: &str = "skill://";

/// Inserted after the frontmatter of every `SKILL.md` served over MCP.
pub const MCP_PREAMBLE: &str = "\
> **Loaded from the Longbridge MCP server.** This skill was written for the `longbridge` CLI, \
but here there is no shell and no CLI. Read every CLI instruction as a description of the \
capability you need, and call the tool from your tool list that provides it.
>
> - Never run, install, or ask the user to install the `longbridge` CLI. Ignore `--help`, \
`--format`, `--yes`, `--execute <CODE>` and `longbridge auth login`: this connection is already \
authorised, and on an auth error the user reconnects the Longbridge connector.
> - Use only tools that are in your tool list. Some capabilities are not available for every \
account, region or connector; when the tool you need is missing, tell the user rather than \
working around it.
> - Writes (orders, fund orders, grid strategies, DCA plans, price alerts) are gated by the \
server. Call the tool once without `execute`: it returns a preview and a `confirmation_code`, \
or the client asks the user with a confirmation form. Show the preview, and call again with \
`execute` set to that code only after the user explicitly confirms that exact preview. The \
preview is the confirmation step: do not ask for a separate confirmation before the first call, \
and do not follow this skill's CLI rule that a write has no dry run.
";

struct Skills {
    source: PathBuf,
    served: PathBuf,
    catalog: SkillCatalog,
}

static SKILLS: OnceLock<Option<Skills>> = OnceLock::new();

fn skills() -> Option<&'static Skills> {
    SKILLS
        .get_or_init(|| {
            let source = PathBuf::from(std::env::var_os(SOURCE_ENV)?);
            let served =
                std::env::temp_dir().join(format!("longbridge-mcp-skills-{}", std::process::id()));
            let loaded = materialize(&source, &served)
                .map_err(|e| e.to_string())
                .and_then(|()| SkillCatalog::from_dir(&served).map_err(|e| e.to_string()));
            match loaded {
                Ok(catalog) => {
                    tracing::info!(
                        ?source,
                        skills = catalog.list_skills().skills.len(),
                        "loaded MCP skills"
                    );
                    Some(Skills {
                        source,
                        served,
                        catalog,
                    })
                }
                Err(error) => {
                    tracing::error!(?source, %error, "failed to load MCP skills");
                    None
                }
            }
        })
        .as_ref()
}

/// Whether this server offers skills at all.
pub fn enabled() -> bool {
    skills().is_some()
}

/// Re-read the skills on the interval named by `LONGBRIDGE_MCP_SKILLS_RELOAD_SECS`.
/// Does nothing when skills or the interval are not configured.
pub fn spawn_reload() {
    let Some(secs) = std::env::var(RELOAD_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&s| s > 0)
    else {
        return;
    };
    let Some(skills) = skills() else {
        return;
    };
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(secs));
        tick.tick().await;
        loop {
            tick.tick().await;
            let result = materialize(&skills.source, &skills.served)
                .map_err(|e| e.to_string())
                .and_then(|()| skills.catalog.reload().map_err(|e| e.to_string()));
            match result {
                Ok(true) => tracing::info!("reloaded MCP skills"),
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(%error, "failed to reload MCP skills; keeping the last good set")
                }
            }
        }
    });
}

/// Copy `source` into `served`, rewriting every `<skill>/SKILL.md` for MCP.
/// Builds the new copy beside the old one and swaps it in, so a failed copy
/// leaves the last good one in place.
fn materialize(source: &Path, served: &Path) -> std::io::Result<()> {
    let next = served.with_extension("next");
    if next.exists() {
        std::fs::remove_dir_all(&next)?;
    }
    copy_tree(source, &next, 0)?;
    if served.exists() {
        std::fs::remove_dir_all(served)?;
    }
    std::fs::rename(&next, served)
}

fn copy_tree(from: &Path, to: &Path, depth: usize) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        // Repo and plugin metadata (`.git`, `.claude-plugin`) are not skills.
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        // `metadata` follows symlinks: in a Kubernetes ConfigMap mount every
        // visible entry is a symlink into the hidden `..data` directory.
        let kind = std::fs::metadata(&src)?.file_type();
        if kind.is_dir() {
            copy_tree(&src, &dst, depth + 1)?;
        } else if kind.is_file() {
            if depth == 1 && name == "SKILL.md" {
                std::fs::write(&dst, for_mcp(&std::fs::read_to_string(&src)?))?;
            } else {
                std::fs::copy(&src, &dst)?;
            }
        }
    }
    Ok(())
}

/// Rewrite one `SKILL.md` for MCP: cut the trigger list from `description`
/// and insert [`MCP_PREAMBLE`] after the frontmatter. A file without
/// frontmatter is left alone; rmcp skips it anyway.
pub fn for_mcp(skill_md: &str) -> String {
    let Some(rest) = skill_md.strip_prefix("---\n") else {
        return skill_md.to_string();
    };
    let Some(end) = rest.find("\n---\n") else {
        return skill_md.to_string();
    };
    let (frontmatter, body) = (&rest[..end], &rest[end + "\n---\n".len()..]);
    format!(
        "---\n{}\n---\n\n{MCP_PREAMBLE}\n{}",
        without_triggers(frontmatter),
        body.trim_start_matches('\n')
    )
}

/// The frontmatter with everything from `Triggers:` to the end of the
/// `description` value removed. Left unchanged if that would empty it.
fn without_triggers(frontmatter: &str) -> String {
    let lines: Vec<&str> = frontmatter.lines().collect();
    let Some(start) = lines.iter().position(|l| l.starts_with("description:")) else {
        return frontmatter.to_string();
    };
    let header = lines[start]["description:".len()..].trim();
    let mut out: Vec<String> = lines[..start].iter().map(|l| (*l).to_string()).collect();
    let mut next = start + 1;
    if header.starts_with('|') || header.starts_with('>') {
        // Block scalar: its lines are the indented (or blank) ones that follow.
        let block_end = lines[next..]
            .iter()
            .position(|l| !(l.is_empty() || l.starts_with(' ')))
            .map_or(lines.len(), |i| next + i);
        let mut kept = Vec::new();
        for line in &lines[next..block_end] {
            if let Some(cut) = line.find("Triggers:") {
                let head = line[..cut].trim_end();
                if !head.trim().is_empty() {
                    kept.push(head.to_string());
                }
                break;
            }
            kept.push((*line).to_string());
        }
        while kept.last().is_some_and(|l| l.trim().is_empty()) {
            kept.pop();
        }
        if kept.is_empty() {
            return frontmatter.to_string();
        }
        out.push(lines[start].to_string());
        out.extend(kept);
        next = block_end;
    } else {
        let Some(cut) = header.find("Triggers:") else {
            return frontmatter.to_string();
        };
        let quote = header.chars().next().filter(|c| *c == '"' || *c == '\'');
        let head = header[..cut].trim_end();
        if head.trim_matches(['"', '\'']).trim().is_empty() {
            return frontmatter.to_string();
        }
        out.push(match quote {
            Some(q) => format!("description: {head}{q}"),
            None => format!("description: {head}"),
        });
    }
    out.extend(lines[next..].iter().map(|l| (*l).to_string()));
    out.join("\n")
}

/// The skill a `skill://<name>/…` URI belongs to.
fn skill_of(uri: &str) -> Option<&str> {
    uri.strip_prefix(URI_SCHEME)?.split('/').next()
}

fn is_mutating(skill: &Skill) -> bool {
    skill
        .frontmatter
        .extra
        .get("metadata")
        .and_then(|m| m.get("risk_level"))
        .and_then(|v| v.as_str())
        == Some("mutating")
}

/// Names of the skills hidden on this connection.
fn hidden(skills: &Skills, read_only: bool) -> Vec<String> {
    if !read_only {
        return Vec::new();
    }
    skills
        .catalog
        .list_skills()
        .skills
        .iter()
        .filter(|s| is_mutating(s))
        .map(|s| s.frontmatter.name.clone())
        .collect()
}

fn is_hidden(hidden: &[String], uri: &str) -> bool {
    skill_of(uri).is_some_and(|name| hidden.iter().any(|h| h == name))
}

fn unknown(uri: &str) -> McpError {
    McpError::invalid_params(format!("unknown skill resource: {uri}"), None)
}

/// `skills/list`. `None` when this server offers no skills.
pub fn list(read_only: bool) -> Option<ListSkillsResult> {
    let skills = skills()?;
    let hidden = hidden(skills, read_only);
    let mut result = skills.catalog.list_skills();
    result.skills.retain(|s| !is_hidden(&hidden, &s.uri));
    crate::metrics::record_skill_request("list", "*");
    Some(result)
}

/// `skills/get`. `None` when this server offers no skills.
pub fn get(uri: &str, read_only: bool) -> Option<Result<GetSkillResult, McpError>> {
    let skills = skills()?;
    if is_hidden(&hidden(skills, read_only), uri) {
        return Some(Err(unknown(uri)));
    }
    let result = skills.catalog.get_skill(uri);
    // Counted only once the skill exists: the URI comes from the client, and
    // an unknown one must not become a new metrics label.
    if result.is_ok() {
        crate::metrics::record_skill_request("get", skill_of(uri).unwrap_or("unknown"));
    }
    Some(result)
}

/// The skill files to add to `resources/list`.
pub fn resources(read_only: bool) -> Vec<Resource> {
    let Some(skills) = skills() else {
        return Vec::new();
    };
    let hidden = hidden(skills, read_only);
    let mut resources = skills.catalog.resources();
    resources.retain(|r| !is_hidden(&hidden, &r.uri));
    resources
}

/// `resources/read` for a `skill://` URI; `None` when it is not one this
/// server serves on this connection.
pub fn read(uri: &str, read_only: bool) -> Option<ReadResourceResult> {
    let skills = skills()?;
    if is_hidden(&hidden(skills, read_only), uri) {
        return None;
    }
    let result = skills.catalog.read_resource(uri)?;
    crate::metrics::record_skill_request("read", skill_of(uri).unwrap_or("unknown"));
    Some(result)
}

/// `resources/directory/read`. `None` when this server offers no skills.
pub fn read_directory(uri: &str, read_only: bool) -> Option<Result<ListResourcesResult, McpError>> {
    let skills = skills()?;
    let hidden = hidden(skills, read_only);
    if is_hidden(&hidden, uri) {
        return Some(Err(unknown(uri)));
    }
    Some(skills.catalog.read_resource_directory(uri).map(|mut dir| {
        dir.resources.retain(|r| !is_hidden(&hidden, &r.uri));
        dir
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "---
name: longbridge-portfolio
description: |
  Account assets, positions and DCA plans via Longbridge.
  Triggers: \"持仓\", \"positions\",
    \"DCA\"
license: MIT
metadata:
  risk_level: mutating
---

# Longbridge Portfolio

Body.
";

    #[test]
    fn block_description_loses_its_triggers_and_gains_the_preamble() {
        let out = for_mcp(BLOCK);
        assert!(!out.contains("Triggers"), "{out}");
        assert!(
            out.contains("  Account assets, positions and DCA plans via Longbridge.\nlicense: MIT")
        );
        assert!(
            out.contains("risk_level: mutating"),
            "the rest of the frontmatter stays"
        );
        let preamble = out
            .find("Loaded from the Longbridge MCP server")
            .expect("preamble");
        assert!(preamble < out.find("# Longbridge Portfolio").expect("body kept"));
        assert!(out.starts_with("---\nname: longbridge-portfolio\n"));
    }

    #[test]
    fn inline_description_is_cut_and_keeps_its_quote() {
        let md =
            "---\nname: x\ndescription: \"Quotes and charts. Triggers: price, 股价\"\n---\nBody\n";
        let out = for_mcp(md);
        assert!(
            out.contains("description: \"Quotes and charts.\"\n"),
            "{out}"
        );
    }

    #[test]
    fn a_description_that_is_only_triggers_is_left_alone() {
        let md = "---\nname: x\ndescription: |\n  Triggers: a, b\n---\nBody\n";
        assert!(for_mcp(md).contains("  Triggers: a, b"));
    }

    #[test]
    fn a_file_without_frontmatter_is_unchanged() {
        assert_eq!(for_mcp("# Just a doc\n"), "# Just a doc\n");
    }

    #[test]
    fn skill_uris_name_their_skill() {
        assert_eq!(
            skill_of("skill://longbridge-watchlist/references/alert.md"),
            Some("longbridge-watchlist")
        );
        assert_eq!(skill_of("skill://longbridge/SKILL.md"), Some("longbridge"));
        assert_eq!(skill_of("lb://tools/depth/output-schema"), None);
        let hidden = vec!["longbridge-portfolio".to_string()];
        assert!(is_hidden(&hidden, "skill://longbridge-portfolio/SKILL.md"));
        assert!(!is_hidden(
            &hidden,
            "skill://longbridge-portfolio-x/SKILL.md"
        ));
    }

    /// A Kubernetes ConfigMap mount: every visible entry is a symlink into a
    /// hidden `..data` directory.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_mount_is_followed() {
        let root = std::env::temp_dir().join(format!("lb-skills-link-{}", std::process::id()));
        let data = root.join("..data/longbridge-portfolio");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("SKILL.md"), BLOCK).unwrap();
        let mount = root.join("mount");
        std::fs::create_dir_all(&mount).unwrap();
        std::os::unix::fs::symlink(&data, mount.join("longbridge-portfolio")).unwrap();
        let served = root.join("served");
        materialize(&mount, &served).unwrap();
        let catalog = SkillCatalog::from_dir(&served).unwrap();
        assert_eq!(
            catalog.list_skills().skills.len(),
            1,
            "symlinked skill must load"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn materialized_catalog_serves_rewritten_skills() {
        let root = std::env::temp_dir().join(format!("lb-skills-test-{}", std::process::id()));
        let source = root.join("src");
        let served = root.join("served");
        std::fs::create_dir_all(source.join("longbridge-portfolio/references")).unwrap();
        std::fs::create_dir_all(source.join(".claude-plugin")).unwrap();
        std::fs::write(source.join("longbridge-portfolio/SKILL.md"), BLOCK).unwrap();
        std::fs::write(source.join("longbridge-portfolio/references/dca.md"), "dca").unwrap();
        std::fs::write(source.join(".claude-plugin/plugin.json"), "{}").unwrap();

        materialize(&source, &served).unwrap();
        // A second run replaces the first copy.
        materialize(&source, &served).unwrap();
        let catalog = SkillCatalog::from_dir(&served).unwrap();
        let listed = catalog.list_skills().skills;
        assert_eq!(listed.len(), 1);
        assert!(is_mutating(&listed[0]));
        assert!(!listed[0].frontmatter.description.contains("Triggers"));
        assert!(!served.join(".claude-plugin").exists());
        assert_eq!(
            std::fs::read_to_string(served.join("longbridge-portfolio/references/dca.md")).unwrap(),
            "dca"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
