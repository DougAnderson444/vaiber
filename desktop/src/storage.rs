//! Native storage
use ui::WalletStorage;

#[derive(Clone, Default)]
pub struct DesktopStorage;

impl WalletStorage for DesktopStorage {
    fn save(&self, key: &str, data: &[u8]) -> Result<(), String> {
        // Use the native file system to save data
        std::fs::write(key, data).map_err(|err| format!("Failed to save data: {:?}", err))
    }

    fn load(&self, key: &str) -> Result<Vec<u8>, String> {
        // Read data from the file system
        std::fs::read(key).map_err(|err| format!("Failed to load data: {:?}", err))
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        // Remove the file from the file system
        std::fs::remove_file(key).map_err(|err| format!("Failed to delete data: {:?}", err))
    }

    fn exists(&self, key: &str) -> bool {
        // Check if the file exists
        std::path::Path::new(key).exists()
    }
}
