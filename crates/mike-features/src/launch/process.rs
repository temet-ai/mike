use std::path::PathBuf;

pub fn alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    let Some(pid) = rustix::process::Pid::from_raw(pid) else {
        return false;
    };
    matches!(
        rustix::process::test_kill_process(pid),
        Ok(()) | Err(rustix::io::Errno::PERM)
    )
}

pub fn claude_child_of(parent: u32) -> Option<u32> {
    children(parent).into_iter().find(|pid| is_claude(*pid))
}

fn is_claude(pid: u32) -> bool {
    name(pid).is_some_and(|name| name == "claude")
}

#[cfg(target_os = "linux")]
pub fn children(pid: u32) -> Vec<u32> {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    tasks
        .flatten()
        .filter_map(|task| std::fs::read_to_string(task.path().join("children")).ok())
        .flat_map(|text| {
            text.split_whitespace()
                .filter_map(|s| s.parse::<u32>().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub fn name(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|comm| comm.trim().to_owned())
}

#[cfg(target_os = "linux")]
pub fn cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(target_os = "macos")]
pub fn children(pid: u32) -> Vec<u32> {
    let Ok(parent) = libc::pid_t::try_from(pid) else {
        return Vec::new();
    };
    let mut pids: Vec<libc::pid_t> = vec![0; 1024];
    let capacity = i32::try_from(pids.len() * std::mem::size_of::<libc::pid_t>()).unwrap_or(0);
    let written = unsafe { libc::proc_listchildpids(parent, pids.as_mut_ptr().cast(), capacity) };
    let count = usize::try_from(written).unwrap_or(0).min(pids.len());
    pids.truncate(count);
    pids.into_iter()
        .filter_map(|child| u32::try_from(child).ok())
        .filter(|child| *child > 0)
        .collect()
}

#[cfg(target_os = "macos")]
pub fn name(pid: u32) -> Option<String> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let mut buffer = [0u8; 256];
    let size = u32::try_from(buffer.len()).ok()?;
    let written = unsafe { libc::proc_name(pid, buffer.as_mut_ptr().cast(), size) };
    let length = usize::try_from(written).ok().filter(|n| *n > 0)?;
    Some(String::from_utf8_lossy(&buffer[..length.min(buffer.len())]).into_owned())
}

#[cfg(target_os = "macos")]
pub fn cwd(pid: u32) -> Option<PathBuf> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_vnodepathinfo>()).ok()?;
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            std::ptr::from_mut(&mut info).cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    let bytes: Vec<u8> = info
        .pvi_cdir
        .vip_path
        .iter()
        .flatten()
        .map(|c| c.to_ne_bytes()[0])
        .take_while(|b| *b != 0)
        .collect();
    (!bytes.is_empty()).then(|| PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn children(_pid: u32) -> Vec<u32> {
    Vec::new()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn name(_pid: u32) -> Option<String> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn cwd(_pid: u32) -> Option<PathBuf> {
    None
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
