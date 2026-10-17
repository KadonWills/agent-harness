//! Run a host process with captured output and a hard deadline.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Captured output beyond this many bytes per stream is dropped.
const CAPTURE_LIMIT: usize = 1 << 20;

#[derive(Debug)]
pub(crate) struct Output {
    /// `None` when the process was killed by a signal or the deadline.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub duration: Duration,
}

/// Spawn `cmd`, wait until it exits or `deadline` passes (then kill it).
pub(crate) fn run(cmd: &mut Command, deadline: Instant) -> std::io::Result<Output> {
    let start = Instant::now();
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let (code, timed_out) = wait(&mut child, deadline)?;
    Ok(Output {
        code,
        stdout: join(stdout),
        stderr: join(stderr),
        timed_out,
        duration: start.elapsed(),
    })
}

fn wait(child: &mut Child, deadline: Instant) -> std::io::Result<(Option<i32>, bool)> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status.code(), false));
        }
        if Instant::now() >= deadline {
            // The process may exit between try_wait and kill; both are fine.
            let _ = child.kill();
            let _ = child.wait();
            return Ok((None, true));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn drain<R: Read + Send + 'static>(stream: Option<R>) -> Option<JoinHandle<Vec<u8>>> {
    let mut stream = stream?;
    Some(thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            let room = CAPTURE_LIMIT.saturating_sub(kept.len());
            kept.extend_from_slice(&buf[..n.min(room)]);
        }
        kept
    }))
}

fn join(handle: Option<JoinHandle<Vec<u8>>>) -> String {
    let bytes = handle.and_then(|h| h.join().ok()).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_exit_code() {
        let out = run(
            Command::new("sh").args(["-c", "echo out; echo err >&2; exit 3"]),
            Instant::now() + Duration::from_secs(10),
        )
        .expect("spawn sh");
        assert_eq!(out.code, Some(3));
        assert_eq!(out.stdout.trim(), "out");
        assert_eq!(out.stderr.trim(), "err");
        assert!(!out.timed_out);
    }

    #[test]
    fn kills_at_the_deadline() {
        let out = run(
            Command::new("sh").args(["-c", "sleep 5"]),
            Instant::now() + Duration::from_millis(200),
        )
        .expect("spawn sh");
        assert!(out.timed_out);
        assert_eq!(out.code, None);
        assert!(out.duration < Duration::from_secs(4));
    }
}
