//! Single instance: a unix socket on Linux and macOS, a named pipe on Windows.
//!
//! Why this exists: Wayland has no global hotkey grab, and COSMIC has no stable
//! GlobalShortcuts portal, so `global-hotkey` cannot work here. The workable
//! path on Linux is to let the compositor own the keybinding and have it run a
//! command. Cold-starting the whole app per screenshot is slower than poking a
//! process that is already up, so the first instance listens and later launches
//! just send it a request and exit.
//!
//! Bind it in COSMIC Settings → Keyboard → Shortcuts → Custom:
//!
//! ```text
//! shotr --capture
//! ```
//!
//! Windows needs the same thing for a different reason: the tray daemon must be
//! a single process, or a second launch puts a second icon in the notification
//! area. Windows has no unix sockets, so that end of it is a named pipe spoken
//! through the Win32 calls directly — the same trade this file already makes
//! for `getuid`, and cheaper than a dependency for seven symbols.

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
    /// Preferences changed a binding.
    ///
    /// It runs in its own process, so the daemon holding the registrations
    /// cannot see the edit. Without this the window would show a hotkey the
    /// daemon has never heard of until the next restart.
    ReloadHotkeys,
}

impl Request {
    fn wire(self) -> &'static str {
        match self {
            Request::Show => "show",
            Request::Capture => "capture",
            Request::ReloadHotkeys => "reload-hotkeys",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "show" => Some(Request::Show),
            "capture" => Some(Request::Capture),
            "reload-hotkeys" => Some(Request::ReloadHotkeys),
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

/// Hand a request to a running instance. `false` means nobody was listening.
///
/// Fire and forget, and a miss is not an error: with no daemon up there is
/// nothing holding hotkeys to reload.
#[cfg(unix)]
pub fn poke(request: Request) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket_path()) else {
        return false;
    };
    let _ = stream.write_all(request.wire().as_bytes());
    true
}

/// Claim the socket, or hand `request` to whoever already has it.
#[cfg(unix)]
pub fn start(request: Request) -> Instance {
    let path = socket_path();

    if poke(request) {
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

/// The same contract over a Win32 named pipe.
///
/// One instance, and that limit *is* the lock: whoever creates the pipe owns
/// the tray, and everyone who cannot create it connects to it instead.
#[cfg(windows)]
mod windows_pipe {
    use super::{Instance, Request, channel};
    use std::ffi::c_void;
    use std::ptr::null_mut;

    type Handle = *mut c_void;

    const PIPE_ACCESS_INBOUND: u32 = 0x0000_0001;
    /// `PIPE_TYPE_BYTE | PIPE_WAIT` — both are zero, named here so the call
    /// below does not read as a bare magic number.
    const PIPE_MODE_BYTE_BLOCKING: u32 = 0;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const OPEN_EXISTING: u32 = 3;
    /// A client that connects in the window between `CreateNamedPipeW` and
    /// `ConnectNamedPipe` makes the latter fail with this — which is a
    /// connection in every sense we care about, not an error.
    const ERROR_PIPE_CONNECTED: u32 = 535;
    /// No request is longer than "capture".
    const BUF: u32 = 64;

    unsafe extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer: u32,
            in_buffer: u32,
            default_timeout: u32,
            security: *mut c_void,
        ) -> Handle;
        fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
        fn DisconnectNamedPipe(pipe: Handle) -> i32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            creation: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn ReadFile(
            file: Handle,
            buffer: *mut u8,
            len: u32,
            read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn WriteFile(
            file: Handle,
            buffer: *const u8,
            len: u32,
            written: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn CloseHandle(object: Handle) -> i32;
        fn GetLastError() -> u32;
    }

    /// `INVALID_HANDLE_VALUE` is `-1` cast to a pointer. Comparing as `isize`
    /// sidesteps building that constant in const context.
    fn usable(handle: Handle) -> bool {
        !handle.is_null() && handle as isize != -1
    }

    /// A `HANDLE` is a raw pointer and so is not `Send`. Exactly one handle
    /// goes to exactly one thread, which then owns it until it closes it.
    struct Pipe(Handle);
    unsafe impl Send for Pipe {}

    /// Named pipes share one machine-wide namespace, so two users signed in at
    /// the same time would otherwise poke each other's daemon. Windows forbids
    /// `\` in a username, the only character that could break the path.
    pub(super) fn pipe_name(user: Option<&str>) -> String {
        match user {
            Some(u) if !u.is_empty() => format!(r"\\.\pipe\shotr-{u}"),
            _ => r"\\.\pipe\shotr".to_string(),
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Hand a request to a running instance. `false` means nobody was listening.
    pub(super) fn poke(request: Request) -> bool {
        let name = wide(&pipe_name(std::env::var("USERNAME").ok().as_deref()));
        let client = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_WRITE,
                0,
                null_mut(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if !usable(client) {
            return false;
        }
        let bytes = request.wire().as_bytes();
        let mut written = 0u32;
        unsafe {
            WriteFile(
                client,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                null_mut(),
            );
            CloseHandle(client);
        }
        true
    }

    pub(super) fn start(request: Request) -> Instance {
        let name = wide(&pipe_name(std::env::var("USERNAME").ok().as_deref()));

        // Someone already listening? Hand the request over and step aside.
        if poke(request) {
            return Instance::Secondary;
        }

        let server = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_INBOUND,
                PIPE_MODE_BYTE_BLOCKING,
                1,
                0,
                BUF,
                0,
                null_mut(),
            )
        };
        if !usable(server) {
            // Can't listen — still run, just without remote requests.
            let (_tx, rx) = channel();
            return Instance::Primary(rx);
        }

        let (tx, rx) = channel();
        let pipe = Pipe(server);
        std::thread::spawn(move || {
            let pipe = pipe;
            loop {
                let connected = unsafe { ConnectNamedPipe(pipe.0, null_mut()) } != 0
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                if !connected {
                    break;
                }

                let mut buf = [0u8; BUF as usize];
                let mut read = 0u32;
                let ok =
                    unsafe { ReadFile(pipe.0, buf.as_mut_ptr(), BUF, &mut read, null_mut()) } != 0;
                unsafe { DisconnectNamedPipe(pipe.0) };

                if ok
                    && let Ok(text) = std::str::from_utf8(&buf[..read as usize])
                    && let Some(req) = Request::parse(text)
                    && tx.send(req).is_err()
                {
                    break; // the app is gone
                }
            }
            unsafe { CloseHandle(pipe.0) };
        });
        Instance::Primary(rx)
    }
}

/// Claim the pipe, or hand `request` to whoever already has it.
#[cfg(windows)]
pub fn start(request: Request) -> Instance {
    windows_pipe::start(request)
}

/// Hand a request to a running instance. `false` means nobody was listening.
#[cfg(windows)]
pub fn poke(request: Request) -> bool {
    windows_pipe::poke(request)
}

#[cfg(test)]
mod wire_tests {
    use super::*;

    #[test]
    fn requests_round_trip_through_the_wire_format() {
        for req in [Request::Show, Request::Capture, Request::ReloadHotkeys] {
            assert_eq!(Request::parse(req.wire()), Some(req));
        }
        assert_eq!(Request::parse("capture\n"), Some(Request::Capture));
        assert_eq!(Request::parse("nonsense"), None);
    }

    #[cfg(windows)]
    #[test]
    fn different_users_never_share_a_pipe() {
        assert_ne!(
            windows_pipe::pipe_name(Some("ann")),
            windows_pipe::pipe_name(Some("bob"))
        );
        // An empty variable is as good as unset, as on the unix side.
        assert_eq!(
            windows_pipe::pipe_name(Some("")),
            windows_pipe::pipe_name(None)
        );
    }
}
