//! Screen color on Wayland through the xdg-desktop-portal ScreenCast interface.
//!
//! The portal asks the user once which screen to share (the choice is remembered with a
//! restore token), hands us a PipeWire remote, and a `gst-launch-1.0 pipewiresrc` child
//! process scales the video down to a tiny thumbnail that we average. Needs GStreamer with
//! the PipeWire plugin at run time (`gst-plugin-pipewire` / `gstreamer1.0-pipewire`).

use crate::screen::ScreenSampler;
use crate::HostError;
use epomaker_core::lighting::Rgb;
use epomaker_core::live::average_color;
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SCREENCAST: &str = "org.freedesktop.portal.ScreenCast";
const THUMB_W: usize = 16;
const THUMB_H: usize = 9;

fn err(e: impl std::fmt::Display) -> HostError {
    HostError::Screen(e.to_string())
}

fn token_file() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("open-epomaker-panel").join("screencast-token")
}

struct Portal {
    conn: Connection,
    sender: String,
    counter: u32,
}

impl Portal {
    /// Calls a portal method that answers through a `Request::Response` signal.
    fn request<B>(
        &mut self,
        method: &str,
        args: impl FnOnce(&str) -> B,
    ) -> Result<HashMap<String, OwnedValue>, HostError>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.counter += 1;
        let token = format!("epomaker{}_{}", std::process::id(), self.counter);
        let path = format!("/org/freedesktop/portal/desktop/request/{}/{token}", self.sender);
        let request = Proxy::new(&self.conn, DEST, path.as_str(), "org.freedesktop.portal.Request").map_err(err)?;
        let mut responses = request.receive_signal("Response").map_err(err)?;
        let body = args(&token);
        let proxy = Proxy::new(&self.conn, DEST, PATH, SCREENCAST).map_err(err)?;
        proxy.call_method(method, &body).map_err(err)?;
        let msg = responses.next().ok_or_else(|| err("portal closed the request"))?;
        let (code, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize().map_err(err)?;
        match code {
            0 => Ok(results),
            1 => Err(HostError::Screen("screen sharing was cancelled".into())),
            _ => Err(HostError::Screen(format!("portal {method} failed ({code})"))),
        }
    }
}

type Options = HashMap<&'static str, Value<'static>>;

fn opts(pairs: Vec<(&'static str, Value<'static>)>) -> Options {
    pairs.into_iter().collect()
}

pub struct PortalSampler {
    latest: Arc<Mutex<Option<Rgb>>>,
    child: Child,
    _conn: Connection,
    _remote: OwnedFd,
}

impl Drop for PortalSampler {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ScreenSampler for PortalSampler {
    fn sample(&mut self) -> Result<Rgb, HostError> {
        if let Ok(Some(status)) = self.child.try_wait() {
            return Err(HostError::Screen(format!("gst-launch-1.0 exited ({status})")));
        }
        Ok(self.latest.lock().unwrap().unwrap_or(Rgb(0)))
    }
}

pub fn sampler() -> Result<PortalSampler, HostError> {
    if Command::new("gst-launch-1.0").arg("--version").stdout(Stdio::null()).status().is_err() {
        return Err(HostError::Screen("gst-launch-1.0 not found (install GStreamer and its PipeWire plugin)".into()));
    }
    let conn = Connection::session().map_err(err)?;
    let sender = conn.unique_name().ok_or_else(|| err("no D-Bus name"))?.trim_start_matches(':').replace('.', "_");
    let mut portal = Portal { conn: conn.clone(), sender, counter: 0 };

    let session_token = format!("epomaker_session{}", std::process::id());
    let created =
        portal.request("CreateSession", |t| {
            (opts(vec![
                ("handle_token", Value::from(t.to_string())),
                ("session_handle_token", Value::from(session_token)),
            ]),)
        })?;
    let session: String = created
        .get("session_handle")
        .and_then(|v| String::try_from(v.clone()).ok())
        .ok_or_else(|| err("no session handle"))?;
    let session_path = OwnedObjectPath::try_from(session.clone()).map_err(err)?;

    let restore = std::fs::read_to_string(token_file()).ok().filter(|s| !s.trim().is_empty());
    let sp = session_path.clone();
    portal.request("SelectSources", move |t| {
        let mut o = vec![
            ("handle_token", Value::from(t.to_string())),
            ("types", Value::from(1u32)), // monitors
            ("multiple", Value::from(false)),
            ("cursor_mode", Value::from(1u32)),  // hidden
            ("persist_mode", Value::from(2u32)), // until revoked
        ];
        if let Some(r) = restore {
            o.push(("restore_token", Value::from(r.trim().to_string())));
        }
        (ObjectPath::from(sp), opts(o))
    })?;
    let sp = session_path.clone();
    let started = portal.request("Start", move |t| {
        (ObjectPath::from(sp), "", opts(vec![("handle_token", Value::from(t.to_string()))]))
    })?;
    if let Some(tok) = started.get("restore_token").and_then(|v| String::try_from(v.clone()).ok()) {
        let f = token_file();
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let _ = std::fs::write(f, tok);
    }
    let streams: Vec<(u32, HashMap<String, OwnedValue>)> = started
        .get("streams")
        .and_then(|v| Vec::<(u32, HashMap<String, OwnedValue>)>::try_from(v.clone()).ok())
        .ok_or_else(|| err("the portal returned no stream"))?;
    let node = streams.first().map(|s| s.0).ok_or_else(|| err("the portal returned no stream"))?;

    let proxy = Proxy::new(&conn, DEST, PATH, SCREENCAST).map_err(err)?;
    let empty: Options = HashMap::new();
    let reply = proxy.call_method("OpenPipeWireRemote", &(ObjectPath::from(session_path), empty)).map_err(err)?;
    let fd: zbus::zvariant::OwnedFd = reply.body().deserialize().map_err(err)?;
    let remote: OwnedFd = fd.into();

    // the child inherits the PipeWire socket as fd 3
    let raw = remote.as_raw_fd();
    let mut cmd = Command::new("gst-launch-1.0");
    cmd.args([
        "-q",
        "pipewiresrc",
        "fd=3",
        &format!("path={node}"),
        "always-copy=true",
        "!",
        "videoconvert",
        "!",
        "videoscale",
        "method=nearest-neighbour",
        "!",
        &format!("video/x-raw,format=RGB,width={THUMB_W},height={THUMB_H}"),
        "!",
        "fdsink",
        "fd=1",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    // SAFETY: only async-signal-safe libc calls between fork and exec.
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(move || {
            if libc::dup2(raw, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(err)?;
    let mut out = child.stdout.take().expect("piped stdout");
    let latest = Arc::new(Mutex::new(None));
    let l = latest.clone();
    std::thread::Builder::new()
        .name("screencast".into())
        .spawn(move || {
            let mut frame = vec![0u8; THUMB_W * THUMB_H * 3];
            while out.read_exact(&mut frame).is_ok() {
                let c = average_color(frame.as_chunks::<3>().0.iter().copied());
                *l.lock().unwrap() = Some(c);
            }
        })
        .map_err(err)?;
    Ok(PortalSampler { latest, child, _conn: conn, _remote: remote })
}
