//! Keeping native libraries' shutdown messages off the user's screen.

/// Sends this process's stdout and stderr to /dev/null until it exits.
#[cfg(unix)]
pub fn silence_output() {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    std::io::stdout().flush().ok();
    std::io::stderr().flush().ok();
    if let Ok(devnull) = std::fs::OpenOptions::new().write(true).open("/dev/null") {
        let fd = devnull.as_raw_fd();
        unsafe {
            libc::dup2(fd, libc::STDOUT_FILENO);
            libc::dup2(fd, libc::STDERR_FILENO);
        }
    }
}

#[cfg(not(unix))]
pub fn silence_output() {}
