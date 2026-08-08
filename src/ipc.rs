//! Single-instance IPC over a unix socket.
//!
//! Why this exists: Wayland has no global hotkey grab, and COSMIC has no stable
//! GlobalShortcuts portal, so `global-hotkey` cannot work here. The workable
//! path on Linux is to let the compositor own the keybinding and have it run a
//! command. Cold-starting the whole app per screenshot is slower than poking a
//! process that is already up, so the first instance listens on a socket and
//! later launches just send it a request and exit.
//!
//! Bind it in COSMIC Settings → Keyboard → Shortcuts → Custom:
//!
//! ```text
//! shotr --capture
//! ```

#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// What a second launch asks the running instance to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Request {
    /// Plain `shotr`: bring the existing window forward. Capturing here would
    /// be wrong — the user asked to *see* the app, not to take a screenshot.
    Show,
    /// `shotr --capture`, i.e. the desktop shortcut.
    Capture,
}

impl Request {
    #[cfg(unix)]
    fn wire(self) -> &'static str {
        match self {
            Request::Show => "show",
            Request::Capture => "capture",
        }
    }

    #[cfg(unix)]
    fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "show" => Some(Request::Show),
            "capture" => Some(Request::Capture),
            _ => None,
        }
    }
}

pub enum Instance {
    /// We own the socket. Requests from later launches arrive on the receiver.
    Primary(Receiver<Request>),
    /// Another instance was already running and has been handed the request.
    Secondary,
}

#[cfg(unix)]
fn socket_path_in(runtime_dir: Option<&str>, uid: u32) -> PathBuf {
    match runtime_dir {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("shotr.sock"),
        // No XDG_RUNTIME_DIR (cron, ssh, minimal sessions): fall back to /tmp,
        // namespaced by uid so two users cannot collide.
        _ => PathBuf::from(format!("/tmp/shotr-{uid}.sock")),
    }
}

#[cfg(unix)]
fn socket_path() -> PathBuf {
    socket_path_in(
        std::env::var("XDG_RUNTIME_DIR").ok().as_deref(),
        // SAFETY: getuid is always safe; it cannot fail and touches no memory.
        unsafe { libc_getuid() },
    )
}

/// `getuid(2)` without pulling in the whole libc crate for one symbol.
#[cfg(unix)]
unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

/// Claim the socket, or hand `request` to whoever already has it.
/// Claim the socket, or hand `request` to whoever already has it.
#[cfg(unix)]
pub fn start(request: Request) -> Instance {
    let path = socket_path();

    if let Ok(mut stream) = UnixStream::connect(&path) {
        let _ = stream.write_all(request.wire().as_bytes());
        return Instance::Secondary;
    }

    // Connect failed, so any socket file here is a leftover from a crash.
    let _ = std::fs::remove_file(&path);

    let Ok(listener) = UnixListener::bind(&path) else {
        // Can't listen — still run, just without remote requests.
        let (_tx, rx) = channel();
        return Instance::Primary(rx);
    };

    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = String::new();
            if stream.read_to_string(&mut buf).is_ok()
                && let Some(req) = Request::parse(&buf)
                && tx.send(req).is_err()
            {
                break; // the app is gone
            }
        }
    });
    Instance::Primary(rx)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_runtime_dir() {
        assert_eq!(
            socket_path_in(Some("/run/user/1000"), 1000),
            PathBuf::from("/run/user/1000/shotr.sock")
        );
    }

    #[test]
    fn falls_back_to_a_uid_namespaced_tmp_path() {
        assert_eq!(
            socket_path_in(None, 1000),
            PathBuf::from("/tmp/shotr-1000.sock")
        );
        // An empty variable is as good as unset.
        assert_eq!(
            socket_path_in(Some(""), 42),
            PathBuf::from("/tmp/shotr-42.sock")
        );
    }

    #[test]
    fn different_users_never_share_a_fallback_socket() {
        assert_ne!(socket_path_in(None, 1000), socket_path_in(None, 1001));
    }

    #[test]
    fn requests_round_trip_through_the_wire_format() {
        for req in [Request::Show, Request::Capture] {
            assert_eq!(Request::parse(req.wire()), Some(req));
        }
        assert_eq!(Request::parse("capture\n"), Some(Request::Capture));
        assert_eq!(Request::parse("nonsense"), None);
    }

    /// The real round trip: a listener, a client poke, a request received.
    #[test]
    fn a_secondary_launch_wakes_the_primary_with_the_right_request() {
        let dir = std::env::temp_dir().join(format!("shotr-ipc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shotr.sock");
        let _ = std::fs::remove_file(&path);

        let listener = UnixListener::bind(&path).unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buf = String::new();
                if stream.read_to_string(&mut buf).is_ok()
                    && let Some(req) = Request::parse(&buf)
                {
                    let _ = tx.send(req);
                }
            }
        });

        for expected in [Request::Show, Request::Capture] {
            let mut client = UnixStream::connect(&path).unwrap();
            client.write_all(expected.wire().as_bytes()).unwrap();
            drop(client);
            let got = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("primary never received the request");
            assert_eq!(got, expected);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Windows has no unix sockets, and the single-instance dance only exists to
/// serve the Linux tray daemon — on Windows a window can hide itself, so every
/// launch simply does its own work.
#[cfg(windows)]
pub fn start(_request: Request) -> Instance {
    let (_tx, rx) = channel();
    Instance::Primary(rx)
}
