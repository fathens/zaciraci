/// Get current process RSS (Resident Set Size) in bytes.
///
/// Returns 0 if measurement is not supported or fails.
#[cfg(target_os = "macos")]
pub fn get_rss_bytes() -> u64 {
    use std::mem;

    unsafe extern "C" {
        fn mach_task_self() -> u32;
    }

    unsafe {
        let mut info = mem::MaybeUninit::<libc::mach_task_basic_info_data_t>::uninit();
        let mut count = (mem::size_of::<libc::mach_task_basic_info_data_t>()
            / mem::size_of::<libc::natural_t>())
            as libc::mach_msg_type_number_t;
        let ret = libc::task_info(
            mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            info.as_mut_ptr().cast(),
            &mut count,
        );
        if ret != libc::KERN_SUCCESS {
            return 0;
        }
        let info = info.assume_init();
        info.resident_size
    }
}

#[cfg(target_os = "linux")]
pub fn get_rss_bytes() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if let Some(val) = line.strip_prefix("VmRSS:") {
            let kb: u64 = val
                .trim()
                .trim_end_matches(" kB")
                .trim()
                .parse()
                .unwrap_or(0);
            return kb * 1024;
        }
    }
    0
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn get_rss_bytes() -> u64 {
    0
}

/// Format bytes as human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
