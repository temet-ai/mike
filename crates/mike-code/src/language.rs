use std::path::Path;

use ast_grep_core::Language;
use ast_grep_language::SupportLang;

pub fn language_for(path: &str, bytes: &[u8]) -> Option<SupportLang> {
    let file = Path::new(path);
    if file.extension().is_some() {
        return by_extension(file);
    }
    by_shebang(bytes)
}

pub fn language_name(language: SupportLang) -> String {
    language.to_string().to_ascii_lowercase()
}

pub fn grammars() -> Vec<String> {
    let mut names: Vec<String> = SupportLang::all_langs()
        .iter()
        .map(|lang| language_name(*lang))
        .collect();
    names.sort();
    names
}

fn by_extension(file: &Path) -> Option<SupportLang> {
    let extension = file.extension()?.to_str()?;
    match extension {
        "ts" | "mts" | "cts" | "js" | "mjs" | "cjs" => Some(SupportLang::TypeScript),
        "tsx" | "jsx" => Some(SupportLang::Tsx),
        "zsh" => Some(SupportLang::Bash),
        _ => SupportLang::from_path(file),
    }
}

fn by_shebang(bytes: &[u8]) -> Option<SupportLang> {
    let first = bytes.split(|b| *b == b'\n').next()?;
    let line = std::str::from_utf8(first).ok()?;
    let interpreter = line.strip_prefix("#!")?.split_whitespace().last()?;
    let name = interpreter.rsplit('/').next()?;
    match name {
        "bash" | "sh" | "zsh" => Some(SupportLang::Bash),
        "node" | "bun" | "deno" | "tsx" => Some(SupportLang::TypeScript),
        "ruby" => Some(SupportLang::Ruby),
        "lua" | "luajit" => Some(SupportLang::Lua),
        "elixir" => Some(SupportLang::Elixir),
        _ if name.starts_with("python") => Some(SupportLang::Python),
        _ => None,
    }
}
