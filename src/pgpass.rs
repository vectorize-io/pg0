//! Reads `.pgpass`-format files the way libpq does: `hostname:port:database:username:password`,
//! `*` in the first four fields, `\:` and `\\` escapes, `#` comment lines, first match wins.

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PgpassError {
    #[error("PGPASSFILE names {}, which cannot be read: {source}", .path.display())]
    Unreadable {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("PGPASSFILE names {}, which group or world can access (mode {mode:04o}). Remove group and world access: chmod 0600 {}", .path.display(), .path.display())]
    Open { path: PathBuf, mode: u32 },
    #[error("PGPASSFILE names {}, which has no entry for this instance", .path.display())]
    NoMatch { path: PathBuf },
}

/// Read the file, check that only its owner can access it, and return the password of the first
/// entry that matches the host, port, database, and username.
pub fn password_from_file(
    path: &Path,
    host: &str,
    port: u16,
    db: &str,
    user: &str,
) -> Result<String, PgpassError> {
    check_permissions(path)?;
    let text = std::fs::read_to_string(path).map_err(|source| PgpassError::Unreadable {
        path: path.to_path_buf(),
        source,
    })?;
    select(&text, host, port, db, user).ok_or_else(|| PgpassError::NoMatch {
        path: path.to_path_buf(),
    })
}

/// One field of a line: the text as written and the text after the escapes are removed.
struct Field {
    raw: String,
    decoded: String,
}

/// Split a line at unescaped colons into at most five fields. The fifth field takes the rest of the line.
fn split_fields(line: &str) -> Vec<Field> {
    let mut fields = Vec::with_capacity(5);
    let (mut raw, mut decoded, mut escaped) = (String::new(), String::new(), false);
    for c in line.chars() {
        if escaped {
            raw.push(c);
            decoded.push(c);
            escaped = false;
        } else if c == '\\' {
            raw.push(c);
            escaped = true;
        } else if c == ':' && fields.len() < 4 {
            fields.push(Field {
                raw: std::mem::take(&mut raw),
                decoded: std::mem::take(&mut decoded),
            });
        } else {
            raw.push(c);
            decoded.push(c);
        }
    }
    fields.push(Field { raw, decoded });
    fields
}

fn field_matches(field: &Field, value: &str) -> bool {
    field.raw == "*" || field.decoded == value
}

/// Return the password of the first line that matches the host, port, database, and username.
pub fn select(text: &str, host: &str, port: u16, db: &str, user: &str) -> Option<String> {
    let port = port.to_string();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = split_fields(line);
        if fields.len() < 5 {
            continue;
        }
        let wanted = [host, port.as_str(), db, user];
        if fields.iter().zip(wanted).all(|(f, v)| field_matches(f, v)) {
            return Some(fields[4].decoded.clone());
        }
    }
    None
}

/// Write a password as the fifth field of a line: `:` becomes `\:` and `\` becomes `\\`.
pub fn escape(pw: &str) -> String {
    pw.replace('\\', "\\\\").replace(':', "\\:")
}

/// Accept a file that only its owner can access. libpq ignores a file with any group or world bit.
#[cfg(unix)]
pub fn check_permissions(path: &Path) -> Result<(), PgpassError> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|source| PgpassError::Unreadable {
        path: path.to_path_buf(),
        source,
    })?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(PgpassError::Open {
            path: path.to_path_buf(),
            mode,
        });
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn check_permissions(_path: &Path) -> Result<(), PgpassError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(text: &str) -> Option<String> {
        select(text, "127.0.0.1", 5433, "app", "me")
    }

    #[cfg(unix)]
    fn unique_dir(prefix: &str) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        std::env::temp_dir().join(format!(
            "{}-{}",
            prefix,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn select_decodes_escaped_colon_and_backslash() {
        assert_eq!(
            pick("127.0.0.1:5433:app:me:a\\:b\\\\c"),
            Some("a:b\\c".to_string())
        );
    }

    #[test]
    fn select_keeps_plain_character_after_unneeded_escape() {
        // libpq reads \a as a.
        assert_eq!(pick("127.0.0.1:5433:app:me:\\ab"), Some("ab".to_string()));
    }

    #[test]
    fn select_matches_wildcard_in_each_field() {
        for line in [
            "*:5433:app:me:pw",
            "127.0.0.1:*:app:me:pw",
            "127.0.0.1:5433:*:me:pw",
            "127.0.0.1:5433:app:*:pw",
        ] {
            assert_eq!(pick(line), Some("pw".to_string()), "{}", line);
        }
    }

    #[test]
    fn select_keeps_star_in_password_literal() {
        assert_eq!(pick("127.0.0.1:5433:app:me:*"), Some("*".to_string()));
    }

    #[test]
    fn select_skips_comment_and_blank_lines() {
        let text = "# 127.0.0.1:5433:app:me:wrong\n\n   \n127.0.0.1:5433:app:me:right\n";
        assert_eq!(pick(text), Some("right".to_string()));
    }

    #[test]
    fn select_handles_crlf_and_missing_final_newline() {
        assert_eq!(
            pick("x:1:y:z:a\r\n127.0.0.1:5433:app:me:pw"),
            Some("pw".to_string())
        );
        assert_eq!(pick("127.0.0.1:5433:app:me:pw\r\n"), Some("pw".to_string()));
    }

    #[test]
    fn select_skips_short_line() {
        assert_eq!(
            pick("127.0.0.1:5433:app:me\n127.0.0.1:5433:app:me:ok"),
            Some("ok".to_string())
        );
        assert_eq!(pick("127.0.0.1:5433:app:me"), None);
    }

    #[test]
    fn select_returns_first_match() {
        assert_eq!(
            pick("*:*:*:*:one\n127.0.0.1:5433:app:me:two"),
            Some("one".to_string())
        );
    }

    #[test]
    fn select_returns_none_on_any_field_mismatch() {
        for line in [
            "10.0.0.1:5433:app:me:p",
            "127.0.0.1:1:app:me:p",
            "127.0.0.1:5433:other:me:p",
            "127.0.0.1:5433:app:you:p",
        ] {
            assert_eq!(pick(line), None, "{}", line);
        }
    }

    #[test]
    fn written_password_reads_back_unchanged() {
        for pw in ["a:b", "a\\b", "::", "\\\\", ":\\:\\", "plain", "tail\\"] {
            let line = format!("127.0.0.1:5433:app:me:{}", escape(pw));
            assert_eq!(pick(&line), Some(pw.to_string()), "{}", pw);
        }
    }

    #[test]
    #[cfg(unix)]
    fn only_owner_only_modes_pass_permission_check() {
        // Windows has no Unix file modes.
        use std::os::unix::fs::PermissionsExt;
        let dir = unique_dir("pg0-pgpass-modes");
        std::fs::create_dir_all(&dir).unwrap();
        for (mode, ok) in [
            (0o600, true),
            (0o400, true),
            (0o640, false),
            (0o604, false),
            (0o644, false),
            (0o660, false),
            (0o666, false),
            (0o610, false),
        ] {
            let f = dir.join(format!("m{:o}", mode));
            std::fs::write(&f, "x").unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(check_permissions(&f).is_ok(), ok, "mode {:o}", mode);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
