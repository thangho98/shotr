//! Listing and capturing individual windows, straight over Wayland.
//!
//! `xcap::Window::all()` returns an empty list on COSMIC, which is easy to
//! misread as "Wayland does not allow this". It does — cosmic-comp advertises
//! all three protocols needed, they are simply newer than what xcap speaks:
//!
//! ```text
//! ext_foreign_toplevel_list_v1                          names the windows
//! ext_foreign_toplevel_image_capture_source_manager_v1  turns one into a source
//! ext_image_copy_capture_manager_v1                     copies it into a buffer
//! ```
//!
//! Capturing a toplevel directly beats cropping a window out of a screenshot:
//! the pixels come from that window's own buffer, so anything stacked on top of
//! it — or hanging off the edge of the screen — still comes out whole and clean.
//!
//! The protocol deliberately offers no geometry, so this cannot drive a
//! hover-over-the-desktop picker. It drives a list instead, which is the trade
//! the protocol makes: you get to name and copy a window, not to locate it.

use image::RgbaImage;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsFd;

use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
    ext_image_copy_capture_manager_v1::{self, ExtImageCopyCaptureManagerV1},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};

pub use crate::winlist::WindowEntry;

struct Entry {
    handle: ExtForeignToplevelHandleV1,
    title: String,
    app_id: String,
    identifier: String,
    closed: bool,
}

#[derive(Default)]
enum Frame {
    #[default]
    Waiting,
    Ready,
    Failed(String),
}

#[derive(Default)]
struct State {
    shm: Option<wl_shm::WlShm>,
    list: Option<ExtForeignToplevelListV1>,
    source_mgr: Option<ExtForeignToplevelImageCaptureSourceManagerV1>,
    capture_mgr: Option<ExtImageCopyCaptureManagerV1>,
    toplevels: Vec<Entry>,

    size: Option<(u32, u32)>,
    formats: Vec<wl_shm::Format>,
    session_done: bool,
    frame: Frame,
}

impl State {
    fn live(&self) -> impl Iterator<Item = &Entry> {
        // A window that closed while we were enumerating is not a candidate.
        self.toplevels.iter().filter(|e| !e.closed)
    }
}

/// True when the compositor speaks the protocols this module needs. Used to
/// decide whether the window list is worth offering at all.
pub fn supported() -> bool {
    connect().map(|(s, _, _)| s.list.is_some() && s.capture_mgr.is_some() && s.source_mgr.is_some())
        .unwrap_or(false)
}

/// Every window the compositor will name, in the order it announced them.
pub fn list() -> Vec<WindowEntry> {
    let Ok((state, _, _)) = connect() else {
        return Vec::new();
    };
    state
        .live()
        .map(|e| WindowEntry {
            title: e.title.clone(),
            app_id: e.app_id.clone(),
            identifier: e.identifier.clone(),
        })
        .collect()
}

/// Capture one window by its stable identifier.
pub fn capture(identifier: &str) -> Result<RgbaImage, String> {
    let (mut state, conn, mut queue) = connect()?;
    let qh = queue.handle();

    let handle = state
        .live()
        .find(|e| e.identifier == identifier)
        .map(|e| e.handle.clone())
        .ok_or_else(|| "That window is gone".to_string())?;

    let source_mgr = state
        .source_mgr
        .clone()
        .ok_or_else(|| "The compositor will not capture by window".to_string())?;
    let capture_mgr = state
        .capture_mgr
        .clone()
        .ok_or_else(|| "The compositor has no ext_image_copy_capture".to_string())?;
    let shm = state
        .shm
        .clone()
        .ok_or_else(|| "No wl_shm".to_string())?;

    let source: ExtImageCaptureSourceV1 = source_mgr.create_source(&handle, &qh, ());
    let session = capture_mgr.create_session(
        &source,
        ext_image_copy_capture_manager_v1::Options::empty(),
        &qh,
        (),
    );

    // The session reports the buffer size and the formats it can write.
    for _ in 0..40 {
        queue.blocking_dispatch(&mut state).map_err(err)?;
        if state.session_done {
            break;
        }
    }
    let (w, h) = state
        .size
        .ok_or_else(|| "The compositor reported no window size".to_string())?;
    if w == 0 || h == 0 {
        return Err("The window has no size".into());
    }
    let format = pick_format(&state.formats)
        .ok_or_else(|| "No usable image format".to_string())?;

    let stride = w as usize * 4;
    let len = stride * h as usize;
    let mut file = shm_file(len).map_err(|e| format!("Could not create shared memory: {e}"))?;
    let pool = shm.create_pool(file.as_fd(), len as i32, &qh, ());
    let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, format, &qh, ());

    let frame = session.create_frame(&qh, ());
    frame.attach_buffer(&buffer);
    frame.damage_buffer(0, 0, w as i32, h as i32);
    frame.capture();
    conn.flush().map_err(err)?;

    for _ in 0..200 {
        queue.blocking_dispatch(&mut state).map_err(err)?;
        if !matches!(state.frame, Frame::Waiting) {
            break;
        }
    }
    match &state.frame {
        Frame::Ready => {}
        Frame::Failed(why) => return Err(format!("The compositor refused to capture: {why}")),
        Frame::Waiting => return Err("Timed out waiting for the frame".into()),
    }

    let mut bytes = vec![0u8; len];
    file.seek(SeekFrom::Start(0)).map_err(err)?;
    file.read_exact(&mut bytes).map_err(err)?;

    frame.destroy();
    buffer.destroy();
    pool.destroy();
    session.destroy();

    Ok(to_rgba(bytes, w, h, format))
}

/// Bind the globals and drain the initial burst of toplevel announcements.
fn connect() -> Result<(State, Connection, wayland_client::EventQueue<State>), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("No Wayland connection: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());

    let mut state = State::default();
    // First round binds the globals; the second lets the toplevel list arrive,
    // and each handle then sends title/app_id/identifier before its `done`.
    for _ in 0..3 {
        queue.roundtrip(&mut state).map_err(err)?;
    }
    Ok((state, conn, queue))
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// A shm-backed scratch file, unlinked immediately so nothing is left behind
/// even if this process is killed mid-capture.
fn shm_file(len: usize) -> std::io::Result<std::fs::File> {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = std::path::Path::new(&dir).join(format!("shotr-shm-{}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    // The fd keeps the storage alive; the name is no longer needed.
    let _ = std::fs::remove_file(&path);
    file.set_len(len as u64)?;
    Ok(file)
}

/// Prefer a format with alpha, fall back to one without.
fn pick_format(offered: &[wl_shm::Format]) -> Option<wl_shm::Format> {
    const WANTED: [wl_shm::Format; 4] = [
        wl_shm::Format::Argb8888,
        wl_shm::Format::Abgr8888,
        wl_shm::Format::Xrgb8888,
        wl_shm::Format::Xbgr8888,
    ];
    WANTED.into_iter().find(|f| offered.contains(f))
}

/// Shm 32-bit formats are little-endian packed words, so `Argb8888` is `B G R A`
/// in memory. Anything without alpha comes back as fully opaque.
fn to_rgba(mut bytes: Vec<u8>, w: u32, h: u32, format: wl_shm::Format) -> RgbaImage {
    let (swap_rb, has_alpha) = match format {
        wl_shm::Format::Argb8888 => (true, true),
        wl_shm::Format::Xrgb8888 => (true, false),
        wl_shm::Format::Abgr8888 => (false, true),
        _ => (false, false),
    };
    for px in bytes.chunks_exact_mut(4) {
        if swap_rb {
            px.swap(0, 2);
        }
        if !has_alpha {
            px[3] = 255;
        }
    }
    RgbaImage::from_raw(w, h, bytes).unwrap_or_else(|| RgbaImage::new(w.max(1), h.max(1)))
}

// ------------------------------------------------------------------ dispatch

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_shm" => {
                state.shm = Some(registry.bind(name, version.min(1), qh, ()));
            }
            "ext_foreign_toplevel_list_v1" => {
                state.list = Some(registry.bind(name, version.min(1), qh, ()));
            }
            "ext_foreign_toplevel_image_capture_source_manager_v1" => {
                state.source_mgr = Some(registry.bind(name, version.min(1), qh, ()));
            }
            "ext_image_copy_capture_manager_v1" => {
                state.capture_mgr = Some(registry.bind(name, version.min(1), qh, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            state.toplevels.push(Entry {
                handle: toplevel,
                title: String::new(),
                app_id: String::new(),
                identifier: String::new(),
                closed: false,
            });
        }
    }

    // The `toplevel` event carries a new object, so it needs a udata factory.
    wayland_client::event_created_child!(State, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(entry) = state
            .toplevels
            .iter_mut()
            .find(|e| e.handle.id() == handle.id())
        else {
            return;
        };
        match event {
            ext_foreign_toplevel_handle_v1::Event::Title { title } => entry.title = title,
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => entry.app_id = app_id,
            ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                entry.identifier = identifier
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => entry.closed = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                state.size = Some((width, height));
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat {
                format: wayland_client::WEnum::Value(f),
            } => state.formats.push(f),
            ext_image_copy_capture_session_v1::Event::Done => state.session_done = true,
            ext_image_copy_capture_session_v1::Event::Stopped => {
                state.session_done = true;
                state.frame = Frame::Failed("the capture session stopped".into());
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_frame_v1::Event::Ready => state.frame = Frame::Ready,
            ext_image_copy_capture_frame_v1::Event::Failed { reason } => {
                state.frame = Frame::Failed(format!("{reason:?}"));
            }
            _ => {}
        }
    }
}

macro_rules! ignore_events {
    ($($t:ty),* $(,)?) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(
                _: &mut Self,
                _: &$t,
                _: <$t as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}

ignore_events!(
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
    ExtImageCaptureSourceV1,
    ExtForeignToplevelImageCaptureSourceManagerV1,
    ExtImageCopyCaptureManagerV1,
);

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn alpha_formats_win_over_opaque_ones() {
        use wl_shm::Format::*;
        assert_eq!(pick_format(&[Xrgb8888, Argb8888]), Some(Argb8888));
        assert_eq!(pick_format(&[Xrgb8888]), Some(Xrgb8888));
        assert_eq!(pick_format(&[]), None);
        // A format we cannot interpret must be declined, not guessed at.
        assert_eq!(pick_format(&[Rgb332]), None);
    }

    /// Shm words are little-endian, so `Argb8888` arrives as B,G,R,A. Getting
    /// this backwards is invisible on grey and glaring on anything else.
    #[test]
    fn argb_is_byte_swapped_into_rgba() {
        // One pixel: B=10 G=20 R=30 A=40 in memory.
        let img = to_rgba(vec![10, 20, 30, 40], 1, 1, wl_shm::Format::Argb8888);
        assert_eq!(img.get_pixel(0, 0).0, [30, 20, 10, 40], "R and B swap");
    }

    #[test]
    fn formats_without_alpha_come_out_opaque() {
        let img = to_rgba(vec![10, 20, 30, 0], 1, 1, wl_shm::Format::Xrgb8888);
        assert_eq!(
            img.get_pixel(0, 0).0,
            [30, 20, 10, 255],
            "a zero alpha byte in Xrgb is padding, not transparency"
        );
    }

    #[test]
    fn abgr_needs_no_swap() {
        let img = to_rgba(vec![10, 20, 30, 40], 1, 1, wl_shm::Format::Abgr8888);
        assert_eq!(img.get_pixel(0, 0).0, [10, 20, 30, 40]);
    }

    #[test]
    fn a_short_buffer_yields_a_blank_image_rather_than_panicking() {
        let img = to_rgba(vec![0; 4], 10, 10, wl_shm::Format::Argb8888);
        assert_eq!((img.width(), img.height()), (10, 10));
    }
}
