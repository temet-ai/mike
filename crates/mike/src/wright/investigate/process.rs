use std::collections::HashMap;
use std::path::Path;

use rustix::process::{Pid, Signal, kill_process, kill_process_group};

fn parent_of(stat: &str) -> Option<i32> {
    let after = stat.rsplit_once(')')?.1;
    after.split_whitespace().nth(1)?.parse().ok()
}

fn process_parents() -> HashMap<i32, i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return HashMap::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
        .filter_map(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            Some((pid, parent_of(&stat)?))
        })
        .collect()
}

pub(crate) fn descendants(root: i32) -> Vec<i32> {
    let parents = process_parents();
    let mut found = Vec::new();
    let mut frontier = vec![root];
    while let Some(parent) = frontier.pop() {
        for (&pid, &ppid) in &parents {
            if ppid == parent && !found.contains(&pid) {
                found.push(pid);
                frontier.push(pid);
            }
        }
    }
    found
}

fn signal(pids: &[i32], signal: Signal) {
    for pid in pids.iter().filter_map(|&pid| Pid::from_raw(pid)) {
        let _ = kill_process(pid, signal);
    }
}

pub(crate) fn kill_tree(root: i32) {
    let tree = descendants(root);
    signal(&tree, Signal::KILL);
    if let Some(pid) = Pid::from_raw(root) {
        let _ = kill_process_group(pid, Signal::KILL);
        let _ = kill_process(pid, Signal::KILL);
    }
}

fn tree(root: i32) -> Vec<i32> {
    let mut all = vec![root];
    all.extend(descendants(root));
    all
}

pub(crate) fn pause_tree(root: i32) {
    signal(&tree(root), Signal::STOP);
}

pub(crate) fn resume_tree(root: i32) {
    signal(&tree(root), Signal::CONT);
}

pub(crate) fn bwrap_alive(pid: i32) -> bool {
    let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    let program = cmdline.split(|b| *b == 0).next().unwrap_or_default();
    Path::new(&*String::from_utf8_lossy(program))
        .file_name()
        .is_some_and(|name| name == "bwrap")
}
