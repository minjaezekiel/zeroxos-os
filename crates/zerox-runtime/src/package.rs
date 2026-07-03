//! Package service — apm (agex package manager) backend.

pub struct PackageService {
    pub installed: Vec<Package>,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub size_bytes: u64,
    pub signed: bool,
    pub deps: Vec<String>,
}

impl PackageService {
    pub fn new() -> Self { Self { installed: Vec::new() } }

    pub fn install(&mut self, pkg: Package) -> Result<(), &'static str> {
        if !pkg.signed { return Err("package is not signed"); }
        log::info!("[pkg] install {} v{} ({} bytes, {} deps)",
            pkg.name, pkg.version, pkg.size_bytes, pkg.deps.len());
        self.installed.push(pkg);
        Ok(())
    }

    pub fn uninstall(&mut self, name: &str) {
        self.installed.retain(|p| p.name != name);
        log::info!("[pkg] uninstall {}", name);
    }

    pub fn list(&self) -> &[Package] { &self.installed }
}
