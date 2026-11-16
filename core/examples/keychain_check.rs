//! Debug-Hilfe: keychain save/load über Prozessgrenzen hinweg testen.
//! Aufruf: cargo run -p nema-core --example keychain_check -- save <dir>
//!         cargo run -p nema-core --example keychain_check -- load <dir>

use nema_core::crypto;
use nema_core::store::VaultStore;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("save");
    let dir = std::path::PathBuf::from(args.get(2).map(|s| s.as_str()).unwrap_or("/tmp/nema-kc-test"));

    println!("keychain_backend_available = {}", crypto::keychain_backend_available());
    match mode {
        "save" => {
            match VaultStore::init(&dir, true, Some("backup-123")) {
                Ok(store) => {
                    println!("init OK, mode = {:?}", store.mode());
                    let key = crypto::keychain_load_key();
                    let prefix = key.map(|k| k.to_hex()[..8].to_string());
                    println!("load after save (gleicher Prozess): {:?}", prefix);
                }
                Err(e) => println!("init FEHLER: {e}"),
            }
        }
        "load" => {
            match crypto::keychain_load_key() {
                Ok(k) => println!("load in neuem Prozess OK, key prefix = {}", &k.to_hex()[..8]),
                Err(e) => println!("load in neuem Prozess FEHLER: {e}"),
            }
            match VaultStore::unlock(&dir) {
                Ok(store) => println!("unlock OK, mode = {:?}", store.mode()),
                Err(e) => println!("unlock FEHLER: {e}"),
            }
        }
        _ => eprintln!("usage: keychain_check [save|load] [dir]"),
    }
}
