use std::path::{Path, PathBuf};

#[test]
fn project_text_does_not_expose_previous_branding() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();

    for path in text_files(&root) {
        let rel = path.strip_prefix(&root).expect("path under manifest root");
        if is_allowed_external_sdk_metadata(rel) {
            continue;
        }

        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", rel.display()));
        for (line_no, line) in content.lines().enumerate() {
            for forbidden in forbidden_previous_branding() {
                if line.contains(&forbidden) && !is_allowed_external_sdk_line(rel, line) {
                    violations.push(format!(
                        "{}:{} contains {forbidden:?}",
                        rel.display(),
                        line_no + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "previous LongPort branding remains:\n{}",
        violations.join("\n")
    );
}

#[test]
fn project_metadata_points_to_longportapp_hosts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo = read(&root, "Cargo.toml");
    let readme = read(&root, "README.md");
    let server = read(&root, "server.json");
    let dockerfile = read(&root, "Dockerfile");

    assert!(cargo.contains("name = \"longport-mcp\""));
    assert!(readme.contains("https://github.com/longportapp/longport-mcp"));
    assert!(readme.contains("https://mcp.longportapp.com"));
    assert!(server.contains("\"name\": \"com.longportapp/mcp\""));
    assert!(server.contains("\"url\": \"https://mcp.longportapp.com\""));
    assert!(server.contains("\"source\": \"https://github.com/longportapp/longport-mcp\""));
    assert!(dockerfile.contains("com.longportapp/mcp"));
    assert!(dockerfile.contains("longport-mcp"));
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

fn forbidden_previous_branding() -> Vec<String> {
    vec![
        format!("{}{}", "Long", "bridge"),
        format!("{}{}", "Long", "Bridge"),
        format!("{}{}", "long", "bridge"),
        format!("{}{}", "LONGBRIDGE", "_"),
        format!("open.{}", previous_domain()),
        format!("openapi.{}", previous_domain()),
        format!("mcp.{}", previous_domain()),
        format!("github.com/{}/", previous_domain()),
        format!("ghcr.io/{}/", previous_domain()),
        format!("{}{}", "长", "桥"),
        format!("{}{}", "長", "橋"),
    ]
}

fn previous_domain() -> String {
    format!("{}{}", "long", "bridge")
}

fn text_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_text_files(root, root, &mut files);
    files.sort();
    files
}

fn collect_text_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read dir {}: {e}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|e| panic!("read dir entry {}: {e}", dir.display()));
        let path = entry.path();
        let rel = path.strip_prefix(root).expect("path under root");
        if should_skip(rel) {
            continue;
        }
        let file_type = entry
            .file_type()
            .unwrap_or_else(|e| panic!("file type {}: {e}", path.display()));
        if file_type.is_dir() {
            collect_text_files(root, &path, files);
        } else if file_type.is_file() && is_text_file(&path) {
            files.push(path);
        }
    }
}

fn should_skip(path: &Path) -> bool {
    path.starts_with(".git") || path.starts_with("target")
}

fn is_text_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if matches!(
        name,
        "LICENSE" | "Makefile" | "README.md" | "LAUNCHGUIDE.md" | ".gitignore"
    ) {
        return true;
    }

    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some(
            "rs" | "toml" | "lock" | "md" | "txt" | "yml" | "yaml" | "json" | "sh" | "ps1" | "html"
        )
    )
}

fn is_allowed_external_sdk_metadata(path: &Path) -> bool {
    path == Path::new("Cargo.lock")
}

fn is_allowed_external_sdk_line(path: &Path, line: &str) -> bool {
    let sdk_name = previous_domain();
    let sdk_repo = format!("github.com/{sdk_name}/openapi.git");
    (path == Path::new("Cargo.toml")
        && line.trim_start().starts_with(&format!("{sdk_name} = "))
        && line.contains(&sdk_repo))
        || (path == Path::new("src/main.rs")
            && line.trim() == format!("extern crate {sdk_name} as longport;"))
}
