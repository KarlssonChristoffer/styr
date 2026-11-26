//! Model types: hosts.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HostState {
    Online,
    Connecting,
    Offline,
}

/// A machine threads can run on: this one, or another one running hubd.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HostInfo {
    pub name: String,
    /// The machine serving the API.
    pub local: bool,
    /// How it is reached: `ssh <alias>` or a URL. Empty for this machine.
    pub address: String,
    pub state: HostState,
    /// The hubd version running there.
    pub version: Option<String>,
    /// Why it is offline.
    pub error: Option<String>,
    /// Added from the app, so it can be removed there; hosts in `hub.json` cannot.
    pub removable: bool,
    /// SSH works but hubd is not there yet: it can be installed from here, or is being.
    pub setup: Option<HostSetup>,
    /// Being put on the tailnet from here, while that runs or after it stopped.
    #[serde(default)]
    pub join: Option<TailnetJoin>,
    /// What this machine is doing to send it a new hubd right now, e.g. "building hubd 1a2b3c4
    /// for aarch64 on PC". None when nothing is being sent.
    #[serde(default)]
    pub sending: Option<String>,
    /// At least this many minutes between two versions this machine sends it on its own
    /// (`HostConfig::push_quiet_mins`); none is no gap. Set with `PATCH /api/hosts/{name}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub push_quiet_mins: Option<u64>,
}

/// `PATCH /api/hosts/{name}` (feature `hosts.settings`): what to change about a machine added from
/// the app. Every field is optional; one left out stays as it is.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HostSettings {
    /// Minutes between two versions this machine sends it on its own; 0 is no gap.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub push_quiet_mins: Option<u64>,
}

/// Putting a machine on the tailnet over SSH: Tailscale installed there, then signed in by the
/// person, with the link Tailscale gives.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TailnetJoin {
    /// What is going on, or why it stopped.
    pub step: String,
    /// Where to sign the machine in to the tailnet; open it and approve.
    pub login_url: Option<String>,
    pub failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HostSetup {
    /// Reachable over SSH, without hubd.
    Missing,
    /// `install-host.sh` is running there; the host's error says its latest step.
    Installing,
    /// It ran and stopped; the host's error says why. Installing again carries on from there.
    Failed,
    /// SSH reaches it but refuses this machine's key, so nothing can be done there. Its login
    /// password, once, puts the key there (`POST /api/hosts/{name}/key`).
    NoKey,
}

/// A host's login password, given once so SSH puts this machine's key there. It goes to ssh and
/// is never kept.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LetInRequest {
    pub password: String,
}

/// Install hubd on a host. `password` is the sudo password there, given once when the host needs
/// root for packages; it is passed to sudo and never kept. `build_here` answers whether this
/// machine may build hubd for the host (and its updates), remembered for all hosts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct InstallRequest {
    #[ts(optional)]
    pub password: Option<String>,
    #[ts(optional)]
    pub build_here: Option<bool>,
}

/// What Install did. With a question set nothing started: `build` asks whether this machine may
/// build hubd for the host (it says what the host is), `sudo` says what the host needs root for.
/// Install again with the answer goes ahead.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InstallAnswer {
    pub sudo: Option<String>,
    pub build: Option<String>,
}

/// A machine to add: an SSH alias from `~/.ssh/config`, or a URL where hubd answers.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewHost {
    pub name: String,
    #[serde(default)]
    #[ts(optional)]
    pub ssh: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub url: Option<String>,
    /// A pairing code from that machine (`hubd pair`), exchanged for a token of its own.
    #[serde(default)]
    #[ts(optional)]
    pub code: Option<String>,
    /// Its SSH password, given once when it does not know this machine's key yet: used to put
    /// the key there, then forgotten.
    #[serde(default)]
    #[ts(optional)]
    pub password: Option<String>,
}

/// What Set up did: the machine added, or a question first (its SSH password, once).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SetUpAnswer {
    pub host: Option<HostInfo>,
    pub password: Option<String>,
}

/// The machines on this machine's tailnet, from `tailscale status`: where new machines are found.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Tailnet {
    /// Tailscale is installed here and connected.
    pub running: bool,
    /// Why there is no list: not installed, logged out, stopped.
    pub error: Option<String>,
    /// This machine's name on the tailnet.
    pub me: Option<String>,
    pub peers: Vec<TailnetPeer>,
}

/// A machine on the home network that answers SSH.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LanMachine {
    pub ip: String,
    /// What its SSH says first, e.g. `SSH-2.0-OpenSSH_9.2p1 Raspbian-2+deb12u3`.
    pub banner: String,
    /// What runs there, from that: `Raspberry Pi OS`, `Ubuntu`; empty when it does not say.
    pub system: String,
    /// The machine in Hub at that address.
    pub machine: Option<String>,
}

/// Another machine on the tailnet.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TailnetPeer {
    /// Its host name, e.g. `clanker`.
    pub name: String,
    /// Its MagicDNS name, e.g. `clanker.tail1234.ts.net`; empty without MagicDNS.
    pub dns: String,
    /// Its tailnet IPv4 address.
    pub ip: Option<String>,
    /// As Tailscale says it: `linux`, `windows`, `macOS`, `iOS`, `android`.
    pub os: String,
    pub online: bool,
    /// hubd can be installed there (Linux or macOS).
    pub supported: bool,
    /// The machine in Hub it already is, by name, SSH alias or address.
    pub machine: Option<String>,
    /// What to give SSH to reach it: an alias from `~/.ssh/config` that points at it, or its
    /// MagicDNS name (its address without MagicDNS).
    pub ssh: String,
    /// Hidden from the "set it up?" nudge.
    pub dismissed: bool,
}

/// A device let in with a pairing code: a phone, a laptop, another machine's Hub.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Device {
    pub id: String,
    pub name: String,
    #[ts(type = "number")]
    pub created_at: i64,
    /// When it last called, since this core started.
    #[ts(type = "number | null")]
    pub last_seen: Option<i64>,
    /// Let in by the phone QR code: it may follow threads and answer them, nothing else.
    #[serde(default)]
    pub phone: bool,
}

/// What the phone's QR code carries: the phone page on this machine's tailnet address, with a
/// one-time code in the part of the link that never leaves the phone.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PhoneLink {
    pub code: String,
    #[ts(type = "number")]
    pub expires_at: i64,
    /// `https://<machine>.<tailnet>.ts.net/phone#pair=<code>`, when Tailscale serves this machine.
    pub link: Option<String>,
    /// The link as a QR code, an SVG image.
    pub qr: Option<String>,
    /// Why there is no link, e.g. Tailscale is not installed.
    pub problem: Option<String>,
}

/// A one-time code that lets one more device in.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PairCode {
    pub code: String,
    #[ts(type = "number")]
    pub expires_at: i64,
    /// Where other devices reach this machine, when Tailscale serves it.
    pub url: Option<String>,
}

/// A pairing code shown by the device that wants in, with the name it goes by.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PairRequest {
    pub code: String,
    pub name: String,
}

/// The answer to a pairing code: a token for that device alone.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Paired {
    pub token: String,
    /// The name of the machine that let it in.
    pub machine: String,
}

/// How a machine is reached by a device that is not this machine, for clients that connect to
/// every machine themselves (`GET /api/machine/routes`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MachineRoutes {
    /// Kept for good (`identity.rs`); the same in `/api/health`.
    pub id: String,
    pub name: String,
    /// Where Tailscale serves this machine over HTTPS, when it does.
    pub url: Option<String>,
}

/// Another machine a phone can follow by itself, with a one-time phone code there when the phone
/// does not have a token for it yet (`POST /api/phone/machines`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PhoneMachine {
    pub id: String,
    pub name: String,
    /// The machine's own HTTPS address on the tailnet.
    pub url: String,
    pub code: Option<String>,
}

/// What the phone already has, so codes are made only for machines it lacks.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PhoneMachinesRequest {
    #[serde(default)]
    pub have: Vec<String>,
}

/// A look at the iPhone simulators on a Mac (`GET /api/simview`, `simview.rs`): which there are,
/// whether one is being watched live, and what the machine can do.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SimView {
    /// Whether this machine has simulators to look at (a Mac with Xcode); screenshots need no more.
    pub available: bool,
    /// Why not, in words.
    #[serde(default)]
    pub reason: Option<String>,
    /// Whether the live picture can be started (Node and npm are there to run the streaming tool).
    #[serde(default)]
    pub live_possible: bool,
    #[serde(default)]
    pub devices: Vec<SimDevice>,
    #[serde(default)]
    pub live: SimLive,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SimDevice {
    pub udid: String,
    pub name: String,
    /// `iOS 26.3`
    pub runtime: String,
    pub booted: bool,
    /// `iphone` or `ipad`.
    pub kind: String,
}

/// The live picture: `off`, `installing` (the streaming tool is fetched once), `booting`,
/// `starting`, `live` or `error` (with `detail`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SimLive {
    pub phase: String,
    #[serde(default)]
    pub udid: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    /// Pictures received per second over the last few seconds.
    #[serde(default)]
    pub fps: Option<f64>,
}

impl Default for SimLive {
    fn default() -> Self {
        Self { phase: "off".into(), udid: None, detail: None, fps: None }
    }
}

/// Whether a machine is ready for work (`GET /api/machine/ready`): its engines there and signed
/// in, git knowing who you are, room on the disk. Each check that is not fine says how to fix it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MachineReady {
    pub checks: Vec<ReadyCheck>,
    /// Who git commits as there, so another machine can be given the same.
    pub git: Option<GitIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReadyCheck {
    /// Stays the same between checks, e.g. `engine:claude` or `git`.
    pub key: String,
    pub label: String,
    pub state: ReadyState,
    /// What was found, in a few words.
    pub detail: String,
    pub fix: Option<ReadyFix>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ReadyState {
    Ok,
    /// Works, but not for long or not fully.
    Warn,
    /// Threads there will fail on it.
    Missing,
}

/// How a check that is not fine gets fixed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum ReadyFix {
    /// The engine's own CLI in a terminal there, where it asks you to sign in.
    SignIn { engine: String },
    /// `hubd sign-in <engine>` in a terminal there (`MachineOpen::SignIn`): a sign-in that
    /// lasts, kept by Hub for the engines that can make one.
    KeepSignedIn { engine: String },
    /// Hub runs the engine's sign-in there and the app shows its link and code (`login.rs`).
    SignInHere { engine: String },
    /// A terminal there, with what to type in it.
    Shell { hint: String },
    /// Give git there a name and email (`POST /api/machine/git-identity`).
    GitIdentity,
}

/// What trying a machine's keys and sign-ins found (`POST /api/machine/credentials/check`). No
/// value of any key is in it, only what each one did when it was used.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CredentialCheck {
    pub rows: Vec<CredentialRow>,
    #[ts(type = "number")]
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CredentialRow {
    /// Stays the same between checks, e.g. `engine:claude` or `api:jev`.
    pub key: String,
    pub label: String,
    /// `engine` (an engine's sign-in or key) or `api` (an API connection's key).
    pub kind: String,
    pub state: CredentialState,
    /// What was found, in a few words.
    pub detail: String,
    /// When a sign-in Hub keeps runs out, in ms, when known.
    #[ts(type = "number | null")]
    pub until: Option<i64>,
    /// The fix, the same as "Ready for work" offers; None when nothing is wrong.
    pub fix: Option<ReadyFix>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CredentialState {
    /// Tried just now and it worked.
    Ok,
    /// There, but the service or the agent refused it.
    Invalid,
    /// Not there: no key, not signed in, or the program is not installed.
    Missing,
    /// Could not be tried (no answer, or took too long).
    Unknown,
}

/// `hubd sign-in` told the core it is done (`POST /api/engines/{engine}/sign-in`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SignInDone {
    pub engine: String,
    /// When the sign-in Hub keeps runs out, in ms. None when the engine saved its own login.
    #[ts(type = "number | null")]
    pub until: Option<i64>,
}

/// Who git commits as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

/// A project packed up to be carried to another machine (`POST /api/folders/pack`): its git
/// history as a bundle, and the ignored env files a clone would not have.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectPack {
    /// Where its origin points, without any token, so the new clone pushes to the same place.
    pub url: Option<String>,
    /// The branch checked out.
    pub branch: Option<String>,
    /// `git bundle` of its branches and tags, base64. None when too big to carry; then the
    /// other machine clones `url`.
    pub bundle: Option<String>,
    pub files: Vec<PackedFile>,
    /// Files with uncommitted changes. They stay where they are, unless the pack was made to
    /// move a thread (`patch` and `untracked`).
    pub dirty: u32,
    /// The commit checked out, for a pack made to move a thread.
    #[serde(default)]
    pub head: Option<String>,
    /// `git diff --binary HEAD`, base64: the tracked files' uncommitted changes.
    #[serde(default)]
    pub patch: Option<String>,
    /// Files git does not track yet and does not ignore.
    #[serde(default)]
    pub untracked: Vec<PackedFile>,
    /// A folder that is not a repository (a Scratch thread's): `files` is everything in it and it
    /// is laid out as a plain folder, without `git init`.
    #[serde(default)]
    pub plain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedFile {
    /// Relative to the project, with `/`.
    pub rel: String,
    /// base64.
    pub data: String,
}

/// `POST /api/folders/unpack`: lay a pack out at `path` on this machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnpackProject {
    pub path: String,
    pub pack: ProjectPack,
    /// Put the repository exactly where the pack was made (its commit, branch and uncommitted
    /// changes) instead of only adding what is missing. A folder with changes of its own is
    /// refused.
    #[serde(default)]
    pub sync: bool,
    /// The folder is where this thread was before it left: take it as it is, laying the moved
    /// files over it and setting aside changes it still holds from before.
    #[serde(default)]
    pub resume: bool,
}

/// `POST /api/folders/bring`: carry the project at `path` on `from` to `target` on `to` (None
/// is the machine asked).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BringProject {
    pub from: Option<String>,
    pub path: String,
    pub to: Option<String>,
    pub target: String,
}

/// `POST /api/threads/move`: move a thread to another machine (`moving.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MoveThread {
    pub thread: ThreadId,
    /// The machine the thread runs on now. None is the machine asked.
    #[serde(default)]
    #[ts(optional)]
    pub from: Option<String>,
    /// The machine it should carry on.
    pub to: String,
    /// A turn is running: stop it to move (it carries on over there). Without this a running
    /// thread is refused.
    #[serde(default)]
    pub stop: bool,
}

/// What moving a thread did.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MovedThread {
    pub thread: ThreadId,
    pub to: String,
    /// The project's folder on the new machine.
    pub folder: String,
    pub via: String,
    /// Uncommitted files that came along.
    pub dirty: u32,
    /// A running turn was cut off and carries on over there.
    pub stopped: bool,
}

/// `POST /api/threads/leaving`: move every thread that is working or waiting on this machine to
/// `to`, which stays on.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Leaving {
    pub to: String,
}

/// What leaving did: the threads that moved and, for each that did not, why.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Left {
    pub moved: Vec<MovedThread>,
    pub failed: Vec<LeftBehind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LeftBehind {
    pub thread: ThreadId,
    pub title: String,
    pub why: String,
}

/// A thread with its whole log, on its way to another machine (`POST /api/threads/export`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadExport {
    pub thread: Thread,
    pub events: Vec<Event>,
    /// The machine it left.
    pub from: String,
    pub pack: ProjectPack,
    /// A running turn was stopped for the export.
    pub stopped: bool,
}

/// What carrying a project did.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Brought {
    pub folder: String,
    /// The files written there, relative to the project: env files, or every file of a project
    /// with nothing committed yet (`via: "files"`).
    pub files: Vec<String>,
    /// Files with uncommitted changes left on the machine it came from.
    pub dirty: u32,
    /// "bundle" when the history came from the other machine, "clone" when it was cloned from
    /// its remote, "existing" when the folder was already there and only files were added,
    /// "files" when it had no commits and no remote, so its files were copied into a new repo.
    pub via: String,
}
