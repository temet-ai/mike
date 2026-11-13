use std::path::Path;

use anyhow::{Context, Result};

pub fn read(path: &Path) -> Result<Vec<(String, String)>> {
    let text = std::fs::read_to_string(path).context("cannot read the selected dotenv import")?;
    parse(&text)
}

fn parse(text: &str) -> Result<Vec<(String, String)>> {
    text.lines().enumerate().filter_map(|(index, line)| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { return None; }
        Some(parse_assignment(line).map_err(|_| anyhow::anyhow!(
            "unsupported dotenv syntax on line {}; expected a literal KEY=value assignment (contents omitted)", index + 1)))
    }).collect()
}

fn parse_assignment(line: &str) -> Result<(String, String)> {
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    let (key, raw) = line.split_once('=').context("missing assignment")?;
    let key = key.trim();
    anyhow::ensure!(valid_key(key), "invalid name");
    Ok((key.to_owned(), parse_value(raw.trim())?))
}

fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_value(raw: &str) -> Result<String> {
    let mut chars = raw.chars().peekable();
    let mut value = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => quoted(&mut chars, c, &mut value)?,
            '\\' => {
                let escaped = chars.next().context("unterminated escape")?;
                anyhow::ensure!(
                    matches!(escaped, '\'' | '"' | '\\' | ' ' | '#' | '$' | '`'),
                    "unsupported escape"
                );
                value.push(escaped);
            }
            c if c.is_whitespace() => {
                let rest: String = chars.collect();
                anyhow::ensure!(
                    rest.trim_start().starts_with('#') || rest.trim().is_empty(),
                    "unquoted whitespace"
                );
                break;
            }
            '$' | '`' | ';' | '|' | '&' | '<' | '>' | '(' | ')' | '\0' => {
                anyhow::bail!("shell syntax")
            }
            c => value.push(c),
        }
    }
    Ok(value)
}

fn quoted(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    quote: char,
    value: &mut String,
) -> Result<()> {
    while let Some(c) = chars.next() {
        if c == quote {
            return Ok(());
        }
        anyhow::ensure!(c != '\0', "NUL is unsupported");
        if c == '\\' && quote == '"' && chars.peek().is_some_and(|next| matches!(next, '\\' | '"'))
        {
            value.push(chars.next().context("unterminated escape")?);
        } else {
            value.push(c);
        }
    }
    anyhow::bail!("unterminated quote")
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn literal_assignments_preserve_shell_metacharacters_and_unicode() -> Result<()> {
        let rows = parse("export KEY='s $HOME `id` \\ # λ'\nOTHER='a'\\''b'\nEMPTY=\n")?;
        assert_eq!(rows[0].1, "s $HOME `id` \\ # λ");
        assert_eq!(rows[1].1, "a'b");
        assert_eq!(rows[2].1, "");
        Ok(())
    }

    #[test]
    fn shell_commands_and_malformed_values_are_rejected_without_echo() -> Result<()> {
        for input in [
            "KEY=$(secret-command)",
            "KEY=one;secret",
            "KEY='secret",
            "source secret",
            "KEY=secret value",
        ] {
            let error = parse(input)
                .err()
                .context("unsupported syntax was accepted")?
                .to_string();
            assert!(error.contains("line 1"));
            assert!(!error.contains("secret"));
        }
        Ok(())
    }
}
