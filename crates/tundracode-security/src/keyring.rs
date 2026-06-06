use anyhow::Result;

pub struct CredentialStore;

impl Default for CredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore {
    pub fn new() -> Self {
        Self
    }

    pub fn get(&self, service: &str, account: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(service, account)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Keyring error: {}", e)),
        }
    }

    pub fn set(&self, service: &str, account: &str, password: &str) -> Result<()> {
        let entry = keyring::Entry::new(service, account)?;
        entry.set_password(password)?;
        Ok(())
    }

    pub fn delete(&self, service: &str, account: &str) -> Result<()> {
        let entry = keyring::Entry::new(service, account)?;
        entry.delete_password()?;
        Ok(())
    }
}
