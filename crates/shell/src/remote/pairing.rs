//! Which phones may control the TV: the PIN on screen, and a token for each paired phone. The
//! Shell and the Remote server share one `Pairings`, so a revoke takes effect on the next request.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const PIN_DIGITS: usize = 6;
/// Bytes of the secret in the Pairing QR code.
const SECRET_BYTES: usize = 16;
/// Challenges handed out and not yet answered; the oldest is dropped past this.
const OPEN_CHALLENGES: usize = 8;
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

/// What the TV shows to pair one phone: the PIN to type, and the secret in the QR code. They live
/// and die together.
struct Offer {
    pin: String,
    secret: [u8; SECRET_BYTES],
}

impl Offer {
    fn new() -> Self {
        let mut secret = [0u8; SECRET_BYTES];
        getrandom::fill(&mut secret).expect("the OS has randomness");
        Self { pin: random_digits(PIN_DIGITS), secret }
    }
}

struct Inner {
    path: PathBuf,
    phones: Vec<Stored>,
    offer: Option<Offer>,
    challenges: Vec<String>,
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

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("the OS has randomness");
    hex(&buffer)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    (text.len().is_multiple_of(2) && text.is_ascii())
        .then(|| {
            (0..text.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
                .collect()
        })
        .flatten()
}

/// HMAC-SHA256 of `role:challenge:nonce` under the QR secret: the page computes the same with
/// `@noble/hashes`. The roles keep one proof from standing in for another.
fn mac(secret: &[u8], role: &str, challenge: &str, nonce: &str) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes any key length");
    mac.update(format!("{role}:{challenge}:{nonce}").as_bytes());
    mac
}

impl Inner {
    fn locked(&self) -> bool {
        self.locked_until.is_some_and(|until| Instant::now() < until)
    }

    /// Counts a wrong guess. Too many in a row replace the PIN and the secret and pause pairing,
    /// so neither can be guessed.
    fn wrong(&mut self) -> PairError {
        self.tries += 1;
        if self.tries >= PIN_TRIES {
            self.offer = Some(Offer::new());
            self.challenges.clear();
            self.tries = 0;
            self.locked_until = Some(Instant::now() + LOCKOUT);
        }
        PairError::WrongPin
    }

    /// Pairs a phone under `token` and ends the offer that paired it.
    fn pair(&mut self, name: &str, token: String) -> Result<(), PairError> {
        let id = self.phones.iter().map(|p| p.id).max().map_or(1, |id| id + 1);
        self.phones.push(Stored { id, name: name.to_string(), token });
        if let Err(e) = self.save() {
            self.phones.pop();
            return Err(PairError::Save(e));
        }
        self.end_offer();
        Ok(())
    }

    /// The PIN and the secret are no longer on screen, or have done their job.
    fn end_offer(&mut self) {
        self.offer = None;
        self.challenges.clear();
        self.tries = 0;
    }

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
            offer: None,
            challenges: Vec::new(),
            tries: 0,
            locked_until: None,
        })))
    }

    /// The PIN and the QR secret (hex) to show on the TV, made when there are none yet.
    pub fn offer(&self) -> (String, String) {
        let mut inner = self.0.lock().unwrap();
        let offer = inner.offer.get_or_insert_with(Offer::new);
        (offer.pin.clone(), hex(&offer.secret))
    }

    /// Stops accepting the PIN and the secret, because the TV no longer shows them.
    pub fn cancel(&self) {
        self.0.lock().unwrap().end_offer();
    }

    /// Pairs a phone that knows the PIN on screen and returns its token.
    pub fn redeem(&self, pin: &str, name: &str) -> Result<String, PairError> {
        let mut inner = self.0.lock().unwrap();
        if inner.locked() {
            return Err(PairError::WrongPin);
        }
        match &inner.offer {
            Some(offer) if offer.pin == pin => {}
            Some(_) => return Err(inner.wrong()),
            None => return Err(PairError::WrongPin),
        }
        let token = random_hex(32);
        inner.pair(name, token.clone())?;
        Ok(token)
    }

    /// Starts a secret handshake: a nonce the phone must answer, once.
    pub fn challenge(&self) -> Result<String, PairError> {
        let mut inner = self.0.lock().unwrap();
        if inner.locked() || inner.offer.is_none() {
            return Err(PairError::WrongPin);
        }
        let challenge = random_hex(16);
        if inner.challenges.len() >= OPEN_CHALLENGES {
            inner.challenges.remove(0);
        }
        inner.challenges.push(challenge.clone());
        Ok(challenge)
    }

    /// Finishes a secret handshake. A phone that proves it knows the QR secret is paired under a
    /// token both sides work out from the secret, so the token never travels; the TV's own proof
    /// is returned for the phone to check. A challenge answers once, right or wrong.
    pub fn prove(
        &self,
        challenge: &str,
        nonce: &str,
        proof: &str,
        name: &str,
    ) -> Result<String, PairError> {
        let mut inner = self.0.lock().unwrap();
        if inner.locked() {
            return Err(PairError::WrongPin);
        }
        let Some(offer) = &inner.offer else { return Err(PairError::WrongPin) };
        let secret = offer.secret;
        // A challenge this TV never handed out (or already used) guesses nothing, so it costs no
        // try: otherwise anyone on the network could burn a phone's tries by evicting its challenge.
        let Some(asked) = inner.challenges.iter().position(|c| c == challenge) else {
            return Err(PairError::WrongPin);
        };
        inner.challenges.remove(asked);
        let proven = nonce.len() <= 64
            && unhex(proof).is_some_and(|proof| {
                mac(&secret, "phone", challenge, nonce).verify_slice(&proof).is_ok()
            });
        if !proven {
            return Err(inner.wrong());
        }
        let token = hex(&mac(&secret, "token", challenge, nonce).finalize().into_bytes());
        inner.pair(name, token)?;
        Ok(hex(&mac(&secret, "tv", challenge, nonce).finalize().into_bytes()))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Worked with Python's hmac module, and asserted by the Remote web app's tests too
    /// (packages/remote/src/pairing/proofs.test.ts), so the two sides cannot drift.
    #[test]
    fn proofs_match_the_remote_web_apps() {
        let secret: Vec<u8> = (0..16).collect();
        let proof = |role| hex(&mac(&secret, role, "c0ffee", "beef").finalize().into_bytes());
        assert_eq!(
            proof("phone"),
            "38f3841968815fc649d7f027c8eb4450fd38974e32c89e1ccc6f54b39209889a"
        );
        assert_eq!(proof("tv"), "e4b4b823435e6476eed9434ed116ce6f4e2e29573a873e028442b499d81c5ffd");
        assert_eq!(
            proof("token"),
            "f28bee0ce3afed378f068ad9586cac825a8f4dbd37b69274153e3f0b1e38cceb"
        );
    }
}
