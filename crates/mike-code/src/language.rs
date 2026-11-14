use std::path::Path;

use ast_grep_language::SupportLang;

pub fn language_for(path: &str, bytes: &[u8]) -> Option<SupportLang> {
    let file = Path::new(path);
    if file.extension().is_some() {
        return by_extension(file);
    }
    by_shebang(bytes)
}

fn by_extension(file: &Path) -> Option<SupportLang> {
    let extension = file.extension()?.to_str()?;
    match extension {
        "rs" => Some(SupportLang::Rust),
        "ts" | "mts" | "cts" | "js" | "mjs" | "cjs" => Some(SupportLang::TypeScript),
        "tsx" | "jsx" => Some(SupportLang::Tsx),
        "sh" | "bash" => Some(SupportLang::Bash),
        "py" => Some(SupportLang::Python),
        _ => None,
    }
}

fn by_shebang(bytes: &[u8]) -> Option<SupportLang> {
    let first = bytes.split(|b| *b == b'\n').next()?;
    let line = std::str::from_utf8(first).ok()?;
    let interpreter = line.strip_prefix("#!")?.split_whitespace().last()?;
    let name = interpreter.rsplit('/').next()?;
    match name {
        "bash" | "sh" | "zsh" => Some(SupportLang::Bash),
        _ if name.starts_with("python") => Some(SupportLang::Python),
        _ => None,
    }
}
