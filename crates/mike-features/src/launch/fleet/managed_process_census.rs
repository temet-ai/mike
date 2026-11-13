#[cfg(any(target_os = "linux", test))]
fn vanished(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH)
}

#[cfg(target_os = "linux")]
pub async fn quiet(group: libc::pid_t) -> Result<bool, String> {
    let mut leader_seen = false;
    for entry in std::fs::read_dir("/proc").map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<libc::pid_t>().ok())
        else {
            continue;
        };
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if vanished(&error) => continue,
            Err(error) => return Err(format!("owned group cleanup cannot be verified: {error}")),
        };
        let (state, process_group) = stat_group(&stat)?;
        leader_seen |= pid == group && process_group == group;
        if process_group == group && !matches!(state, 'Z' | 'X') {
            return Ok(false);
        }
    }
    if !leader_seen {
        return Err("owned group leader missing from process census".into());
    }
    Ok(true)
}

#[cfg(target_os = "macos")]
pub async fn quiet(group: libc::pid_t) -> Result<bool, String> {
    let mut command = tokio::process::Command::new("/bin/ps");
    command
        .args(["-A", "-o", "pid=", "-o", "pgid=", "-o", "state="])
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|error| format!("owned group census could not start: {error}"))?;
    let output = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait_with_output())
        .await
        .map_err(|_| "owned group census timed out")?
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "owned group census failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|error| error.to_string())?;
    ps_quiet(text, group)
}

#[cfg(any(target_os = "linux", test, feature = "test-seams"))]
pub fn stat_group(stat: &str) -> Result<(char, libc::pid_t), String> {
    let (_, tail) = stat.rsplit_once(')').ok_or("malformed process stat")?;
    let mut fields = tail.split_whitespace();
    let state = fields
        .next()
        .and_then(|value| value.chars().next())
        .ok_or("missing process state")?;
    let _parent = fields.next().ok_or("missing process parent")?;
    let group = fields
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or("missing process group")?;
    Ok((state, group))
}

#[cfg(any(target_os = "macos", test, feature = "test-seams"))]
pub fn ps_quiet(text: &str, group: libc::pid_t) -> Result<bool, String> {
    let mut leader_seen = false;
    let mut live = false;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 3 {
            return Err("malformed owned process census row".into());
        }
        let pid: libc::pid_t = fields[0].parse().map_err(|_| "invalid census pid")?;
        let pgid: libc::pid_t = fields[1].parse().map_err(|_| "invalid census group")?;
        let state = fields[2].chars().next().ok_or("invalid census state")?;
        leader_seen |= pid == group && pgid == group;
        live |= pgid == group && state != 'Z';
    }
    if !leader_seen {
        return Err("owned group leader missing from process census".into());
    }
    Ok(!live)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_that_exits_while_its_stat_is_read_is_gone_not_unverifiable() {
        assert!(vanished(&std::io::Error::from_raw_os_error(libc::ESRCH)));
        assert!(vanished(&std::io::Error::from(
            std::io::ErrorKind::NotFound
        )));
        assert!(!vanished(&std::io::Error::from_raw_os_error(libc::EACCES)));
    }

    #[test]
    fn process_names_and_zombie_rows_preserve_exact_group_identity() -> anyhow::Result<()> {
        assert_eq!(
            stat_group("123 (a (b) c) S 12 123 123 0").map_err(anyhow::Error::msg)?,
            ('S', 123)
        );
        assert!(stat_group("bad").is_err());
        assert!(stat_group("123 (name) S 1 nope").is_err());
        assert!(ps_quiet("123 123 Z\n124 123 Z+\n900 900 R", 123).map_err(anyhow::Error::msg)?);
        assert!(!ps_quiet("123 123 Z\n124 123 S", 123).map_err(anyhow::Error::msg)?);
        assert!(!ps_quiet("123 123 ?", 123).map_err(anyhow::Error::msg)?);
        assert!(ps_quiet("", 123).is_err());
        assert!(ps_quiet("123 nope Z", 123).is_err());
        assert!(ps_quiet("124 123 Z", 123).is_err());
        Ok(())
    }
}
