use super::files::MARKER;
use super::spec::Spec;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn string(text: &str) -> String {
    format!("<string>{}</string>", escape(text))
}

fn entry(out: &mut String, indent: &str, key: &str, value: &str) {
    out.push_str(&format!("{indent}<key>{}</key>\n", escape(key)));
    out.push_str(&format!("{indent}{value}\n"));
}

fn program_arguments(spec: &Spec) -> String {
    let mut out = String::from("<array>\n");
    for argument in spec.program_arguments() {
        out.push_str(&format!("    {}\n", string(&argument)));
    }
    out.push_str("  </array>");
    out
}

fn environment(spec: &Spec) -> String {
    let mut out = String::from("<dict>\n");
    for (name, value) in &spec.environment {
        entry(&mut out, "    ", name, &string(value));
    }
    out.push_str("  </dict>");
    out
}

pub(crate) fn render(spec: &Spec) -> String {
    let log = spec.log_path().display().to_string();
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
    out.push_str(&format!(
        "<!-- {MARKER}. Run `mike service install` to regenerate this file and `mike service uninstall` to remove it. -->\n"
    ));
    out.push_str("<plist version=\"1.0\">\n<dict>\n");
    entry(&mut out, "  ", "Label", &string(spec.kind.launchd_label()));
    entry(&mut out, "  ", "ProgramArguments", &program_arguments(spec));
    entry(
        &mut out,
        "  ",
        "WorkingDirectory",
        &string(&spec.state_dir.display().to_string()),
    );
    entry(&mut out, "  ", "EnvironmentVariables", &environment(spec));
    entry(&mut out, "  ", "RunAtLoad", "<true/>");
    entry(&mut out, "  ", "KeepAlive", "<true/>");
    entry(&mut out, "  ", "StandardOutPath", &string(&log));
    entry(&mut out, "  ", "StandardErrorPath", &string(&log));
    out.push_str("</dict>\n</plist>\n");
    out
}
