use uuid::Uuid;

#[cfg(target_os = "macos")]
pub fn attested(pid: i32, session: Uuid, run: &str) -> bool {
    let Some(mut bytes) = process_arguments(pid) else {
        return false;
    };
    let matched = markers_match(&bytes, session, run);
    bytes.fill(0);
    matched
}

#[cfg(target_os = "macos")]
fn process_arguments(pid: i32) -> Option<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut length = 0;
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || length < std::mem::size_of::<i32>() || length > argument_limit()? {
        return None;
    }
    let mut bytes = vec![0_u8; length];
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || length > bytes.len() {
        bytes.fill(0);
        return None;
    }
    bytes.resize(length, 0);
    Some(bytes)
}

#[cfg(target_os = "macos")]
fn argument_limit() -> Option<usize> {
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    let mut limit: libc::c_int = 0;
    let mut length = std::mem::size_of_val(&limit);
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            std::ptr::from_mut(&mut limit).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    (result == 0 && length == std::mem::size_of_val(&limit))
        .then(|| {
            usize::try_from(limit)
                .ok()?
                .checked_add(std::mem::size_of::<i32>())
        })
        .flatten()
}

pub fn markers_match(bytes: &[u8], session: Uuid, run: &str) -> bool {
    environment(bytes).is_some_and(|env| super::environment_matches(env, session, run))
}

fn environment(bytes: &[u8]) -> Option<&[u8]> {
    let (mut rest, argc) = arguments(bytes)?;
    for _ in 0..argc {
        rest = after_string(rest)?;
    }
    let end = rest
        .windows(2)
        .position(|pair| pair == [0, 0])
        .map_or(rest.len(), |index| index + 1);
    rest.get(..end)
}

fn arguments(bytes: &[u8]) -> Option<(&[u8], usize)> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    let argc = usize::try_from(argc).ok()?;
    if argc == 0 || argc > bytes.len() {
        return None;
    }
    let rest = after_string(bytes.get(4..)?)?;
    let rest = rest.get(rest.iter().position(|byte| *byte != 0)?..)?;
    Some((rest, argc))
}

fn after_string(bytes: &[u8]) -> Option<&[u8]> {
    let end = bytes.iter().position(|byte| *byte == 0)?;
    bytes.get(end + 1..)
}
