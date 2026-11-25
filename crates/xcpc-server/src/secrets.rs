//! Where the model service's API key lives: the macOS keychain, elsewhere a file only the
//! user can read next to the preferences (or memory, for headless runs). `XCPC_LLM_API_KEY`
//! overrides both.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use xcpc_runtime::Error;

fn err(e: impl std::fmt::Display) -> Error {
    Error {
        code: "keychain".into(),
        message: e.to_string(),
    }
}

#[derive(Clone)]
pub struct Secrets {
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    file: Option<PathBuf>,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    memory: Arc<Mutex<Option<String>>>,
}

#[cfg(target_os = "macos")]
const SERVICE: &str = "com.xcpc.workstation";
#[cfg(target_os = "macos")]
const ACCOUNT: &str = "llm-api-key";

impl Secrets {
    pub fn new(preferences: Option<&Path>) -> Self {
        Self {
            file: preferences.map(|p| p.with_file_name("llm-api-key")),
            memory: Arc::default(),
        }
    }

    pub fn get(&self) -> Result<Option<String>, Error> {
        if let Some(k) = std::env::var("XCPC_LLM_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
        {
            return Ok(Some(k));
        }
        self.load()
    }

    #[cfg(target_os = "macos")]
    fn load(&self) -> Result<Option<String>, Error> {
        use security_framework::os::macos::keychain::SecKeychain;
        let keychain = SecKeychain::default().map_err(err)?;
        match keychain.find_generic_password(SERVICE, ACCOUNT) {
            Ok((password, _)) => String::from_utf8(password.to_owned())
                .map(Some)
                .map_err(err),
            Err(_) => Ok(None),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn set(&self, value: &str) -> Result<(), Error> {
        use security_framework::os::macos::keychain::SecKeychain;
        SecKeychain::default()
            .and_then(|k| k.set_generic_password(SERVICE, ACCOUNT, value.as_bytes()))
            .map_err(err)
    }

    #[cfg(target_os = "macos")]
    pub fn clear(&self) -> Result<(), Error> {
        use security_framework::os::macos::keychain::SecKeychain;
        let keychain = SecKeychain::default().map_err(err)?;
        if let Ok((_, item)) = keychain.find_generic_password(SERVICE, ACCOUNT) {
            item.delete();
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn load(&self) -> Result<Option<String>, Error> {
        match &self.file {
            Some(f) => match std::fs::read_to_string(f) {
                Ok(s) => Ok(Some(s.trim().to_owned()).filter(|s| !s.is_empty())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(err(e)),
            },
            None => Ok(self.memory.lock().unwrap().clone()),
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set(&self, value: &str) -> Result<(), Error> {
        let Some(f) = &self.file else {
            *self.memory.lock().unwrap() = Some(value.to_owned());
            return Ok(());
        };
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        let tmp = f.with_extension("tmp");
        {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            let mut file = opts.open(&tmp).map_err(err)?;
            file.write_all(value.as_bytes()).map_err(err)?;
        }
        std::fs::rename(tmp, f).map_err(err)
    }

    #[cfg(not(target_os = "macos"))]
    pub fn clear(&self) -> Result<(), Error> {
        match &self.file {
            Some(f) => match std::fs::remove_file(f) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(err(e)),
                _ => Ok(()),
            },
            None => {
                *self.memory.lock().unwrap() = None;
                Ok(())
            }
        }
    }
}
