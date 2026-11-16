//! Verschlüsselung: AES-256-GCM mit einem Masterkey, der im OS-Schlüsselbund
//! liegt. Fallback ohne Schlüsselbund: Schlüssel wird per Argon2id aus einem
//! Master-Passwort abgeleitet.
//!
//! Dateiformat: `NEM1` || 12-Byte-Nonce || Ciphertext+Tag

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

pub const KEY_SIZE: usize = 32;
const NONCE_SIZE: usize = 12;
const MAGIC: &[u8; 4] = b"NEM1";

pub const KEYCHAIN_SERVICE: &str = "nema-anonymizer";
pub const KEYCHAIN_ACCOUNT: &str = "master-key-v1";

const VERIFY_PLAINTEXT: &[u8] = b"nema-verify-v1";

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("kein OS-Schlüsselbund verfügbar: {0}")]
    KeychainUnavailable(String),
    #[error("Fehler im OS-Schlüsselbund: {0}")]
    Keychain(String),
    #[error("Entschlüsselung fehlgeschlagen — falscher Schlüssel oder beschädigte Daten")]
    Decrypt,
    #[error("ungültiges oder beschädigtes Datenformat")]
    Format,
}

#[derive(Clone)]
pub struct MasterKey(Zeroizing<[u8; KEY_SIZE]>);

impl MasterKey {
    pub fn random() -> Self {
        let mut key = Zeroizing::new([0u8; KEY_SIZE]);
        OsRng.fill_bytes(key.as_mut());
        Self(key)
    }

    pub fn from_bytes(bytes: [u8; KEY_SIZE]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }

    pub fn from_hex(s: &str) -> Result<Self, CryptoError> {
        let bytes = hex::decode(s).map_err(|_| CryptoError::Format)?;
        let arr: [u8; KEY_SIZE] = bytes.try_into().map_err(|_| CryptoError::Format)?;
        Ok(Self::from_bytes(arr))
    }
}

/// Prüft, ob ein OS-Schlüsselbund prinzipiell funktioniert (Backend vorhanden).
/// `true` auch dann, wenn dort noch kein Schlüssel liegt.
pub fn keychain_backend_available() -> bool {
    match keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(entry) => matches!(entry.get_password(), Ok(_) | Err(keyring::Error::NoEntry)),
        Err(_) => false,
    }
}

pub fn keychain_load_key() -> Result<MasterKey, CryptoError> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| CryptoError::KeychainUnavailable(e.to_string()))?;
    let hex_key = entry.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => {
            CryptoError::KeychainUnavailable("kein Schlüssel im Schlüsselbund".into())
        }
        other => CryptoError::Keychain(other.to_string()),
    })?;
    MasterKey::from_hex(&hex_key)
}

pub fn keychain_save_key(key: &MasterKey) -> Result<(), CryptoError> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| CryptoError::KeychainUnavailable(e.to_string()))?;
    entry
        .set_password(&key.to_hex())
        .map_err(|e| CryptoError::Keychain(e.to_string()))
}

pub fn keychain_delete_key() -> Result<(), CryptoError> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| CryptoError::KeychainUnavailable(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CryptoError::Keychain(e.to_string())),
    }
}

/// Verifiziert, dass der Schlüsselbund den Key auch wirklich persistiert
/// (liefert ihn zurück). Captured kaputte/flüchtige Backends.
pub fn keychain_roundtrip_ok(key: &MasterKey) -> bool {
    if keychain_save_key(key).is_err() {
        return false;
    }
    matches!(keychain_load_key(), Ok(k) if k.to_hex() == key.to_hex())
}

/// Envelope: Masterkey mit einem passwort-abgeleiteten KEK verschlüsseln.
pub fn wrap_key(kek: &MasterKey, key: &MasterKey) -> Vec<u8> {
    encrypt(kek, key.as_bytes())
}

pub fn unwrap_key(kek: &MasterKey, data: &[u8]) -> Result<MasterKey, CryptoError> {
    let plain = decrypt(kek, data)?;
    let arr: [u8; KEY_SIZE] = plain.try_into().map_err(|_| CryptoError::Decrypt)?;
    Ok(MasterKey::from_bytes(arr))
}

/// Leitet einen Schlüssel per Argon2id aus einem Passwort ab (64 MiB, t=3, p=1).
pub fn derive_key_from_password(password: &str, salt: &[u8; 16]) -> MasterKey {
    let params = Params::new(65536, 3, 1, Some(KEY_SIZE)).expect("gültige Argon2-Parameter");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; KEY_SIZE]);
    argon2
        .hash_password_into(password.as_bytes(), salt, out.as_mut())
        .expect("Argon2-Fehler");
    MasterKey(out)
}

fn cipher(key: &MasterKey) -> Aes256Gcm {
    Aes256Gcm::new_from_slice(key.as_bytes()).expect("Schlüssellänge ist fix 32 Byte")
}

pub fn encrypt(key: &MasterKey, plaintext: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ct = cipher(key)
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .expect("Verschlüsselung schlägt nicht fehl");
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_SIZE + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    out
}

pub fn decrypt(key: &MasterKey, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if data.len() < MAGIC.len() + NONCE_SIZE + 16 || &data[..MAGIC.len()] != MAGIC {
        return Err(CryptoError::Format);
    }
    let nonce = Nonce::from_slice(&data[MAGIC.len()..MAGIC.len() + NONCE_SIZE]);
    let ct = &data[MAGIC.len() + NONCE_SIZE..];
    cipher(key).decrypt(nonce, ct).map_err(|_| CryptoError::Decrypt)
}

pub fn seal_json<T: Serialize + ?Sized>(key: &MasterKey, value: &T) -> Result<Vec<u8>, CryptoError> {
    let json = serde_json::to_vec(value).map_err(|_| CryptoError::Format)?;
    Ok(encrypt(key, &json))
}

pub fn open_json<T: DeserializeOwned>(key: &MasterKey, data: &[u8]) -> Result<T, CryptoError> {
    let plain = decrypt(key, data)?;
    serde_json::from_slice(&plain).map_err(|_| CryptoError::Format)
}

/// Baut den Verifier für das Passwort-Modus: Versiegelter bekannter Klartext,
/// an dem ein falsches Passwort erkannt wird.
pub fn make_verifier(key: &MasterKey) -> String {
    hex::encode(encrypt(key, VERIFY_PLAINTEXT))
}

pub fn check_verifier(key: &MasterKey, verifier_hex: &str) -> bool {
    match hex::decode(verifier_hex) {
        Ok(data) => decrypt(key, &data).map(|p| p == VERIFY_PLAINTEXT).unwrap_or(false),
        Err(_) => false,
    }
}

pub fn new_salt() -> [u8; 16] {
    let mut s = [0u8; 16];
    OsRng.fill_bytes(&mut s);
    s
}

pub fn salt_to_b64(s: &[u8; 16]) -> String {
    B64.encode(s)
}

pub fn salt_from_b64(s: &str) -> Result<[u8; 16], CryptoError> {
    let v = B64.decode(s).map_err(|_| CryptoError::Format)?;
    v.try_into().map_err(|_| CryptoError::Format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = MasterKey::random();
        let data = encrypt(&key, b"hallo welt");
        let back = decrypt(&key, &data).unwrap();
        assert_eq!(back, b"hallo welt");
    }

    #[test]
    fn wrong_key_fails() {
        let k1 = MasterKey::random();
        let k2 = MasterKey::random();
        let data = encrypt(&k1, b"geheim");
        assert!(matches!(decrypt(&k2, &data), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn tamper_detected() {
        let key = MasterKey::random();
        let mut data = encrypt(&key, b"geheim");
        let n = data.len();
        data[n - 1] ^= 1;
        assert!(decrypt(&key, &data).is_err());
    }

    #[test]
    fn garbage_is_format_error() {
        let key = MasterKey::random();
        assert!(matches!(decrypt(&key, b"xx"), Err(CryptoError::Format)));
        assert!(matches!(decrypt(&key, b""), Err(CryptoError::Format)));
    }

    #[test]
    fn password_derive_deterministic() {
        let salt = new_salt();
        let a = derive_key_from_password("geheim123", &salt);
        let b = derive_key_from_password("geheim123", &salt);
        assert_eq!(a.to_hex(), b.to_hex());
        let c = derive_key_from_password("geheim124", &salt);
        assert_ne!(a.to_hex(), c.to_hex());
    }

    #[test]
    fn wrap_unwrap_roundtrip() {
        let key = MasterKey::random();
        let salt = new_salt();
        let kek = derive_key_from_password("backup-passwort", &salt);
        let wrapped = wrap_key(&kek, &key);
        let back = unwrap_key(&kek, &wrapped).unwrap();
        assert_eq!(back.to_hex(), key.to_hex());
        // falscher KEK schlägt fehl
        let wrong = derive_key_from_password("anderes-passwort", &salt);
        assert!(unwrap_key(&wrong, &wrapped).is_err());
    }

    #[test]
    fn json_roundtrip_and_verifier() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Demo {
            name: String,
            n: u32,
        }
        let key = MasterKey::random();
        let v = Demo { name: "Müller".into(), n: 42 };
        let sealed = seal_json(&key, &v).unwrap();
        let back: Demo = open_json(&key, &sealed).unwrap();
        assert_eq!(v, back);

        let verifier = make_verifier(&key);
        assert!(check_verifier(&key, &verifier));
        assert!(!check_verifier(&MasterKey::random(), &verifier));
    }
}
