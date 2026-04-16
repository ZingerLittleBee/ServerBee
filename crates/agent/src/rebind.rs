use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::Context;

fn render_token_content(existing: &str, token: &str) -> String {
    let token_line = format!("token = \"{token}\"");
    let mut lines: Vec<String> = existing.lines().map(ToOwned::to_owned).collect();

    if let Some(pos) = lines.iter().position(|line| is_token_line(line)) {
        lines[pos] = token_line;
    } else {
        lines.push(token_line);
    }

    lines.join("\n")
}

fn is_token_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.strip_prefix("token") else {
        return false;
    };

    rest.trim_start().starts_with('=')
}

pub fn persist_rebind_token(path: impl AsRef<Path>, token: &str) -> anyhow::Result<()> {
    let path = path.as_ref();
    let existing = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?
    } else {
        String::new()
    };
    let rendered = render_token_content(&existing, token);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().unwrap_or_else(|| OsStr::new("agent.toml"));
    let temp_path = parent.join(format!(
        ".{}.rebind.{}.tmp",
        file_name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));

    let write_result = (|| -> anyhow::Result<()> {
        let mut temp_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| format!("failed to create {}", temp_path.display()))?;
        temp_file
            .write_all(rendered.as_bytes())
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        temp_file
            .sync_all()
            .with_context(|| format!("failed to sync {}", temp_path.display()))?;
        if path.exists() {
            if let Ok(metadata) = fs::metadata(path) {
                let _ = fs::set_permissions(&temp_path, metadata.permissions());
            }
        }
        fs::rename(&temp_path, path).with_context(|| {
            format!("failed to atomically replace {} with {}", path.display(), temp_path.display())
        })?;

        #[cfg(unix)]
        {
            if let Some(dir) = path.parent() {
                if let Ok(dir_file) = fs::File::open(dir) {
                    let _ = dir_file.sync_all();
                }
            }
        }

        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    write_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn persist_rebind_token_replaces_existing_token_line_without_touching_other_lines() {
        let tempdir = TempDir::new().expect("tempdir");
        let path = tempdir.path().join("agent.toml");
        fs::write(
            &path,
            r#"server_url = "http://127.0.0.1:9527"
token = "old-token"
log.level = "debug""#,
        )
        .expect("seed file");

        persist_rebind_token(&path, "new-token").expect("persist");

        let content = fs::read_to_string(&path).expect("read file");
        assert_eq!(
            content,
            r#"server_url = "http://127.0.0.1:9527"
token = "new-token"
log.level = "debug""#
        );
    }

    #[test]
    fn persist_rebind_token_appends_token_line_when_missing() {
        let tempdir = TempDir::new().expect("tempdir");
        let path = tempdir.path().join("agent.toml");
        fs::write(&path, "server_url = \"http://127.0.0.1:9527\"\n").expect("seed file");

        persist_rebind_token(&path, "fresh-token").expect("persist");

        let content = fs::read_to_string(&path).expect("read file");
        assert_eq!(
            content,
            r#"server_url = "http://127.0.0.1:9527"
token = "fresh-token""#
        );
    }
}
