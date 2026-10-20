//! Which phones may control the TV: the PIN on screen, and a token for each paired phone. The
//! Shell and the Remote server share one `Pairings`, so a revoke takes effect on the next request.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

const PIN_DIGITS: usize = 6;
/// Wrong PINs tolerated before the PIN on screen is replaced.
const PIN_TRIES: u32 = 5;
/// How long pairing is refused after the PIN is replaced, so guessing gets nowhere on a LAN.
const LOCKOUT: Duration = Duration::from_secs(30);

/// A paired phone, as the Shell lists it. The token never leaves `Pairings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phone {
    pub id: u64,
    pub name: String,
}

#[derive(Debug)]
pub enum PairError {
    WrongPin,
    Save(io::Error),
}

#[derive(Clone, Serialize, Deserialize)]
struct Stored {
    id: u64,
    name: String,
    token: String,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    phone: Vec<Stored>,
}

struct Inner {
    path: PathBuf,
    phones: Vec<Stored>,
    pin: Option<String>,
    tries: u32,
    locked_until: Option<Instant>,
}

#[derive(Clone)]
pub struct Pairings(Arc<Mutex<Inner>>);

/// `digits` random decimal digits.
fn random_digits(digits: usize) -> String {
    let mut bytes = vec![0u8; digits];
    getrandom::fill(&mut bytes).expect("the OS has randomness");
    bytes.iter().map(|b| char::from(b'0' + b % 10)).collect()
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS has randomness");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Inner {
    fn save(&self) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file =
            toml::to_string(&File { phone: self.phones.clone() }).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("toml.tmp");
        std::fs::write(&tmp, file)?;
        super::identity::restrict(&tmp)?;
        std::fs::rename(&tmp, &self.path)
    }
}

impl Pairings {
    /// Reads the store. A missing or damaged file means no phone is paired: booting beats keeping
    /// a broken file.
    pub fn load(path: &Path) -> Self {
        let file: File = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        Self(Arc::new(Mutex::new(Inner {
            path: path.to_path_buf(),
            phones: file.phone,
            pin: None,
            tries: 0,
            locked_until: None,
        })))
    }

    /// The PIN to show on the TV, made when there is none yet.
    pub fn pin(&self) -> String {
        let mut inner = self.0.lock().unwrap();
        inner.pin.get_or_insert_with(|| random_digits(PIN_DIGITS)).clone()
    }

    /// Stops accepting the PIN, because the TV no longer shows it.
    pub fn cancel(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.pin = None;
        inner.tries = 0;
    }

    /// Pairs a phone that knows the PIN on screen and returns its token. Too many wrong PINs in
    /// a row replace the PIN, so it cannot be guessed.
    pub fn redeem(&self, pin: &str, name: &str) -> Result<String, PairError> {
        let mut inner = self.0.lock().unwrap();
        if inner.locked_until.is_some_and(|until| Instant::now() < until) {
            return Err(PairError::WrongPin);
        }
        let Some(shown) = inner.pin.clone() else { return Err(PairError::WrongPin) };
        if pin != shown {
            inner.tries += 1;
            if inner.tries >= PIN_TRIES {
                inner.pin = Some(random_digits(PIN_DIGITS));
                inner.tries = 0;
                inner.locked_until = Some(Instant::now() + LOCKOUT);
            }
            return Err(PairError::WrongPin);
        }
        let token = random_token();
        let id = inner.phones.iter().map(|p| p.id).max().map_or(1, |id| id + 1);
        inner.phones.push(Stored { id, name: name.to_string(), token: token.clone() });
        if let Err(e) = inner.save() {
            inner.phones.pop();
            return Err(PairError::Save(e));
        }
        inner.pin = None;
        inner.tries = 0;
        Ok(token)
    }

    /// Whether `token` belongs to a paired phone.
    pub fn check(&self, token: &str) -> bool {
        self.0.lock().unwrap().phones.iter().any(|p| p.token == token)
    }

    pub fn phones(&self) -> Vec<Phone> {
        let inner = self.0.lock().unwrap();
        inner.phones.iter().map(|p| Phone { id: p.id, name: p.name.clone() }).collect()
    }

    /// Unpairs a phone: its token is refused from the next request on.
    pub fn revoke(&self, id: u64) -> io::Result<()> {
        let mut inner = self.0.lock().unwrap();
        let before = inner.phones.len();
        inner.phones.retain(|p| p.id != id);
        if inner.phones.len() != before { inner.save() } else { Ok(()) }
    }

    /// Unpairs every phone. Returns whether there were any.
    pub fn clear(&self) -> io::Result<bool> {
        let mut inner = self.0.lock().unwrap();
        let had = !inner.phones.is_empty();
        inner.phones.clear();
        if had { inner.save().map(|()| true) } else { Ok(false) }
    }
}
