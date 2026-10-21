//! The Engine as castLabs Electron in a child process (ADR 0006). The Shell spawns the host script
//! (`engine/host`), restarts it when it dies, and talks to it over a Unix socket (a named pipe on
//! Windows) in newline-delimited JSON. Frames are bitmaps the host copies into files in the frames
//! directory (`/dev/shm` on the device); a message says which one is new.

use std::{
    collections::VecDeque,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{Notify, oneshot},
};

use crate::{
    apps::{AppDefinition, page_script},
    engine::{Cdm, Engine, Frame, Session},
    shell::Key,
};

/// The frames the host rotates through, so the Shell never reads the file being written.
const SLOTS: u8 = 3;

/// What starts the host, and where it and the Shell meet.
pub struct EngineConfig {
    /// Electron, or whatever stands in for it.
    pub program: PathBuf,
    /// The arguments before the ones the Shell adds: for Electron, the host script's folder.
    pub args: Vec<OsString>,
    /// Added to the host's environment, which already has `M4TV_SOCKET`, `M4TV_FRAMES` and
    /// `M4TV_DATA`.
    pub env: Vec<(OsString, OsString)>,
    /// The Unix socket's path, or the named pipe's name on Windows.
    pub endpoint: String,
    pub frames_dir: PathBuf,
    /// Where the Apps keep their cookies and storage (one partition each).
    pub data_dir: PathBuf,
    /// The size the page is laid out at.
    pub view: (u32, u32),
    /// How long to wait before starting a host that died again.
    pub restart_delay: Duration,
    /// How long a host may take to connect before it is taken for hung and started again.
    pub connect_timeout: Duration,
}

impl EngineConfig {
    /// A host for this device: socket in the runtime directory, frames in `/dev/shm` where there is
    /// one.
    pub fn new(program: PathBuf, args: Vec<OsString>, data_dir: PathBuf) -> Self {
        let id = std::process::id();
        let temp = std::env::temp_dir();
        Self {
            program,
            args,
            env: Vec::new(),
            endpoint: default_endpoint(id),
            frames_dir: if Path::new("/dev/shm").is_dir() {
                PathBuf::from(format!("/dev/shm/m4tv-frames-{id}"))
            } else {
                temp.join(format!("m4tv-frames-{id}"))
            },
            data_dir,
            // shortcut: a fixed 720p page; the Pi build lays it out at the screen's size.
            view: (1280, 720),
            restart_delay: Duration::from_secs(1),
            connect_timeout: Duration::from_secs(30),
        }
    }
}

#[cfg(unix)]
fn default_endpoint(id: u32) -> String {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    dir.join(format!("m4tv-engine-{id}.sock")).to_string_lossy().into_owned()
}

#[cfg(windows)]
fn default_endpoint(id: u32) -> String {
    format!(r"\\.\pipe\m4tv-engine-{id}")
}

#[cfg(unix)]
mod endpoint {
    use std::{io, os::unix::fs::PermissionsExt};

    use tokio::net::{UnixListener, UnixStream};

    pub struct Endpoint(UnixListener, String);

    impl Endpoint {
        pub fn bind(path: &str) -> io::Result<Self> {
            let _ = std::fs::remove_file(path);
            let listener = UnixListener::bind(path)?;
            // A shared temp directory is the fallback when there is no runtime directory.
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self(listener, path.into()))
        }

        pub async fn accept(&mut self) -> io::Result<UnixStream> {
            Ok(self.0.accept().await?.0)
        }
    }

    impl Drop for Endpoint {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.1);
        }
    }
}

#[cfg(windows)]
mod endpoint {
    use std::io;

    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    pub struct Endpoint {
        name: String,
        next: NamedPipeServer,
    }

    impl Endpoint {
        pub fn bind(name: &str) -> io::Result<Self> {
            let next = ServerOptions::new().first_pipe_instance(true).create(name)?;
            Ok(Self { name: name.into(), next })
        }

        /// A pipe instance serves one client, so the next one is made as this one is handed out.
        pub async fn accept(&mut self) -> io::Result<NamedPipeServer> {
            self.next.connect().await?;
            Ok(std::mem::replace(&mut self.next, ServerOptions::new().create(&self.name)?))
        }
    }
}

use endpoint::Endpoint;

#[derive(Serialize)]
struct LaunchApp<'a> {
    id: &'a str,
    start_url: &'a str,
    user_agent: Option<&'a str>,
    /// What runs in every page: spatial navigation, then the App's own CSS and JS.
    script: String,
}

#[derive(Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum ToHost<'a> {
    Launch { session: u64, app: LaunchApp<'a>, width: u32, height: u32 },
    Close,
    Key { key: &'a str },
    Text { text: &'a str },
    Back,
    /// Fetch the CDM again.
    #[serde(rename = "cdm_retry")]
    CdmRetry,
}

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum FromHost {
    Ready {},
    Frame { session: u64, slot: u8, width: u32, height: u32 },
    State { session: u64, can_go_back: bool, editable: bool },
    /// "preparing", "ready" or "failed" with a reason.
    Cdm { status: String, reason: Option<String> },
}

fn line(message: &ToHost) -> String {
    serde_json::to_string(message).expect("a message is JSON")
}

struct State {
    cdm: Cdm,
    session: Session<()>,
    can_go_back: bool,
    /// The message that opened the App now open: sent again to a host that has just started.
    launch: Option<String>,
    /// Whether a host is connected. Commands are only queued for one that is: what a host that is
    /// not there would have been told is stale by the time the next one starts.
    connected: bool,
    outbox: VecDeque<String>,
    /// The running host's process id.
    host: Option<u32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            // Nothing is ready before the first host says so.
            cdm: Cdm::Preparing,
            session: Session::default(),
            can_go_back: false,
            launch: None,
            connected: false,
            outbox: VecDeque::new(),
            host: None,
        }
    }
}

/// What the engine and its thread share.
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// Something is waiting in the outbox.
    wake: Notify,
}

impl Shared {
    fn queue(&self, message: &ToHost) {
        let mut state = self.state.lock().unwrap();
        if state.connected {
            state.outbox.push_back(line(message));
            self.wake.notify_one();
        }
    }
}

pub struct ElectronEngine {
    shared: Arc<Shared>,
    view: (u32, u32),
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl ElectronEngine {
    /// Starts the host and keeps it running until the engine is dropped.
    pub fn start(mut config: EngineConfig) -> io::Result<Self> {
        // The host, in another working directory's reach, is given these.
        config.frames_dir = std::path::absolute(&config.frames_dir)?;
        config.data_dir = std::path::absolute(&config.data_dir)?;
        std::fs::create_dir_all(&config.frames_dir)?;
        std::fs::create_dir_all(&config.data_dir)?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        // Bound here, so a second Shell on the same endpoint is an error and not a silent race.
        let endpoint = {
            let _guard = runtime.enter();
            Endpoint::bind(&config.endpoint)?
        };
        let shared = Arc::new(Shared::default());
        let (stop, stopped) = oneshot::channel();
        let view = config.view;
        let thread = {
            let shared = shared.clone();
            std::thread::Builder::new().name("m4tv-engine".into()).spawn(move || {
                runtime.block_on(async {
                    tokio::select! {
                        () = supervise(&config, endpoint, &shared) => {}
                        _ = stopped => {}
                    }
                });
                // Dropping the runtime drops the host's Child, which kills it.
                let _ = std::fs::remove_dir_all(&config.frames_dir);
            })?
        };
        Ok(Self { shared, view, stop: Some(stop), thread: Some(thread) })
    }

    /// Whether a host is connected: it is not between one dying and the next starting.
    pub fn connected(&self) -> bool {
        self.shared.state.lock().unwrap().connected
    }

    /// The process id of the host that is running, if one is.
    pub fn host_id(&self) -> Option<u32> {
        self.shared.state.lock().unwrap().host
    }
}

impl Drop for ElectronEngine {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Engine for ElectronEngine {
    fn cdm(&self) -> Cdm {
        self.shared.state.lock().unwrap().cdm.clone()
    }

    fn retry_cdm(&mut self) {
        self.shared.state.lock().unwrap().cdm = Cdm::Preparing;
        self.shared.queue(&ToHost::CdmRetry);
    }

    fn open(&mut self, app: &AppDefinition) {
        let mut state = self.shared.state.lock().unwrap();
        let (session, _) = state.session.begin();
        state.can_go_back = false;
        let launch = line(&ToHost::Launch {
            session,
            app: LaunchApp {
                id: &app.id,
                start_url: &app.start_url,
                user_agent: app.user_agent.as_deref(),
                script: page_script(app),
            },
            width: self.view.0,
            height: self.view.1,
        });
        state.launch = Some(launch.clone());
        if state.connected {
            state.outbox.push_back(launch);
            self.shared.wake.notify_one();
        }
    }

    fn close(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.session.end();
        state.can_go_back = false;
        state.launch = None;
        drop(state);
        self.shared.queue(&ToHost::Close);
    }

    fn key(&mut self, key: Key) {
        if let Some(key) = key.electron_key_code() {
            self.shared.queue(&ToHost::Key { key });
        }
    }

    fn can_go_back(&self) -> bool {
        self.shared.state.lock().unwrap().can_go_back
    }

    fn go_back(&mut self) {
        self.shared.queue(&ToHost::Back);
    }

    fn take_frame(&mut self) -> Option<Frame> {
        self.shared.state.lock().unwrap().session.take_frame()
    }

    fn editable_focused(&self) -> bool {
        self.shared.state.lock().unwrap().session.editable()
    }

    fn type_text(&mut self, text: &str) {
        self.shared.queue(&ToHost::Text { text });
    }
}

/// Runs the host, and runs it again whenever it or its connection ends.
async fn supervise(config: &EngineConfig, mut endpoint: Endpoint, shared: &Shared) {
    loop {
        match spawn(config) {
            Ok(mut host) => {
                shared.state.lock().unwrap().host = host.id();
                tokio::select! {
                    served = async {
                        let stream = tokio::time::timeout(config.connect_timeout, endpoint.accept())
                            .await
                            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "it never connected"))??;
                        serve(stream, config, shared).await
                    } => {
                        if let Err(e) = served {
                            eprintln!("the Engine host's connection failed: {e}");
                        }
                    }
                    status = host.wait() => eprintln!("the Engine host exited ({status:?})"),
                }
                {
                    let mut state = shared.state.lock().unwrap();
                    state.connected = false;
                    // The page is gone with its host and comes back on the next one.
                    state.can_go_back = false;
                    state.session.forget_page_state();
                    // The next host starts its fetch again.
                    state.cdm = Cdm::Preparing;
                }
                let _ = host.kill().await;
                shared.state.lock().unwrap().host = None;
            }
            Err(e) => eprintln!("could not start the Engine host {:?}: {e}", config.program),
        }
        tokio::time::sleep(config.restart_delay).await;
    }
}

fn spawn(config: &EngineConfig) -> io::Result<Child> {
    Command::new(&config.program)
        .args(&config.args)
        .envs(config.env.iter().map(|(k, v)| (k, v)))
        .env("M4TV_SOCKET", &config.endpoint)
        .env("M4TV_FRAMES", &config.frames_dir)
        .env("M4TV_DATA", &config.data_dir)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
}

/// Talks to one connected host until it hangs up.
async fn serve(
    stream: impl AsyncRead + AsyncWrite,
    config: &EngineConfig,
    shared: &Shared,
) -> io::Result<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    {
        // The App that is open is launched again, as the first thing this host hears.
        let mut state = shared.state.lock().unwrap();
        state.connected = true;
        state.outbox = state.launch.iter().cloned().collect();
    }
    loop {
        let outgoing: Vec<String> = shared.state.lock().unwrap().outbox.drain(..).collect();
        for message in outgoing {
            send_line(&mut writer, &message).await?;
        }
        tokio::select! {
            incoming = lines.next_line() => match incoming? {
                Some(incoming) => receive(&incoming, config, &shared.state),
                None => return Ok(()),
            },
            () = shared.wake.notified() => {}
        }
    }
}

async fn send_line(writer: &mut (impl AsyncWrite + Unpin), text: &str) -> io::Result<()> {
    writer.write_all(text.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}

fn receive(text: &str, config: &EngineConfig, state: &Mutex<State>) {
    let Ok(message) = serde_json::from_str::<FromHost>(text) else {
        eprintln!("the Engine host said something unknown: {text}");
        return;
    };
    match message {
        FromHost::Ready {} => {}
        FromHost::Frame { session, slot, width, height } => {
            if slot >= SLOTS || !state.lock().unwrap().session.is_current(session) {
                return;
            }
            let wanted = (width as usize).checked_mul(height as usize * 4);
            let path = config.frames_dir.join(format!("frame-{slot}.bgra"));
            match std::fs::read(path) {
                Ok(bgra) if Some(bgra.len()) == wanted => {
                    state.lock().unwrap().session.paint(session, Frame { width, height, bgra });
                }
                Ok(_) => eprintln!("a frame from the Engine host is not {width}x{height}"),
                Err(e) => eprintln!("could not read a frame from the Engine host: {e}"),
            }
        }
        FromHost::Cdm { status, reason } => {
            state.lock().unwrap().cdm = match status.as_str() {
                "ready" => Cdm::Ready,
                "failed" => Cdm::Failed(reason.unwrap_or_else(|| "it failed".into())),
                _ => Cdm::Preparing,
            };
        }
        FromHost::State { session, can_go_back, editable } => {
            let mut state = state.lock().unwrap();
            if state.session.is_current(session) {
                state.can_go_back = can_go_back;
                state.session.set_editable(session, editable);
            }
        }
    }
}
