//! Run a host process with captured output and a hard deadline.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
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

/// How long to wait for output after a kill: grandchildren that escaped
/// the kill may hold the pipes open.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Spawn `cmd`, wait until it exits or `deadline` passes (then kill it and
/// everything it spawned).
pub(crate) fn run(cmd: &mut Command, deadline: Instant) -> std::io::Result<Output> {
    let start = Instant::now();
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group, so a timeout can kill grandchildren too.
        cmd.process_group(0);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let (code, timed_out) = wait(&mut child, deadline)?;
    let duration = start.elapsed();
    let grace = if timed_out {
        Some(Instant::now() + DRAIN_GRACE)
    } else {
        None
    };
    Ok(Output {
        code,
        stdout: collect(stdout, grace),
        stderr: collect(stderr, grace),
        timed_out,
        duration,
    })
}

fn wait(child: &mut Child, deadline: Instant) -> std::io::Result<(Option<i32>, bool)> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status.code(), false));
        }
        if Instant::now() >= deadline {
            // The process may exit between try_wait and kill; both are fine.
            kill_group(child);
            let _ = child.kill();
            let _ = child.wait();
            return Ok((None, true));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn kill_group(child: &Child) {
    // std has no safe API to signal a process group; `kill` is POSIX.
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{}", child.id())])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(unix))]
fn kill_group(_: &Child) {}

fn drain<R: Read + Send + 'static>(stream: Option<R>) -> Option<Receiver<Vec<u8>>> {
    let mut stream = stream?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            let room = CAPTURE_LIMIT.saturating_sub(kept.len());
            kept.extend_from_slice(&buf[..n.min(room)]);
        }
        let _ = tx.send(kept);
    });
    Some(rx)
}

/// Wait for a drained stream; after a kill, give up at `grace`.
fn collect(rx: Option<Receiver<Vec<u8>>>, grace: Option<Instant>) -> String {
    let bytes = rx
        .and_then(|rx| match grace {
            Some(until) => rx
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .ok(),
            None => rx.recv().ok(),
        })
        .unwrap_or_default();
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
        // `sleep` is a grandchild that inherits the pipes; it must die too,
        // or collecting its output would block until it exits.
        let start = Instant::now();
        let out = run(
            Command::new("sh").args(["-c", "sleep 5; echo late"]),
            Instant::now() + Duration::from_millis(200),
        )
        .expect("spawn sh");
        assert!(out.timed_out);
        assert_eq!(out.code, None);
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );
        assert!(!out.stdout.contains("late"));
    }
}
