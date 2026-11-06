use super::files::MARKER;
use super::spec::{ServiceKind, Spec};

fn specifiers(text: &str) -> String {
    text.replace('%', "%%")
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn exec_word(word: &str) -> String {
    let word = specifiers(word).replace('$', "$$");
    if word
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        quoted(&word)
    } else {
        word
    }
}

fn environment_line(name: &str, value: &str) -> String {
    format!(
        "Environment={}\n",
        quoted(&specifiers(&format!("{name}={value}")))
    )
}

pub(crate) fn render(spec: &Spec) -> String {
    let log = specifiers(&spec.log_path().display().to_string());
    let mut out = format!(
        "# {MARKER}. Run `mike service install` to regenerate this file and `mike service uninstall` to remove it.\n"
    );
    out.push_str(&format!(
        "[Unit]\nDescription={}\n",
        spec.kind.description()
    ));
    if spec.kind == ServiceKind::Sidecar {
        out.push_str(&format!("After={}\n", ServiceKind::Proxy.systemd_unit()));
    }
    let command: Vec<String> = spec
        .program_arguments()
        .iter()
        .map(|word| exec_word(word))
        .collect();
    out.push_str("\n[Service]\nType=simple\n");
    out.push_str(&format!("ExecStart={}\n", command.join(" ")));
    out.push_str(&format!(
        "WorkingDirectory={}\n",
        specifiers(&spec.state_dir.display().to_string())
    ));
    for (name, value) in &spec.environment {
        out.push_str(&environment_line(name, value));
    }
    out.push_str("Restart=on-failure\nRestartSec=3\n");
    out.push_str(&format!(
        "StandardOutput=append:{log}\nStandardError=append:{log}\n"
    ));
    out.push_str("\n[Install]\nWantedBy=default.target\n");
    out
}
