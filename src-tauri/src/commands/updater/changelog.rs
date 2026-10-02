#[derive(serde::Serialize)]
pub struct ChangelogEntry {
    pub version: String,
    pub content: String,
}

pub(crate) fn extract_changelog_for_version(changelog: &str, version: &str) -> Option<String> {
    let lines: Vec<&str> = changelog.lines().collect();
    let mut in_section = false;
    let mut content = Vec::new();
    let target = format!("## [{}]", version);

    for line in lines {
        if line.starts_with(&target) {
            in_section = true;
            continue;
        }

        if in_section {
            if line.starts_with("## [") {
                break;
            }
            if !line.is_empty() {
                content.push(line);
            }
        }
    }

    if content.is_empty() {
        None
    } else {
        Some(content.join("\n").trim().to_string())
    }
}

#[tauri::command]
pub fn get_changelog(version: String) -> Result<ChangelogEntry, String> {
    let changelog_content = include_str!("../../../../CHANGELOG.md");

    let changelog_text = extract_changelog_for_version(changelog_content, &version)
        .ok_or_else(|| format!("No changelog entry found for version {}", version))?;

    Ok(ChangelogEntry {
        version,
        content: changelog_text,
    })
}
