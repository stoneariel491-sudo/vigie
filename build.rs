//! Embarque l'icone et l'identite dans l'executable.
//!
//! Sans l'icone, elle ne vit que sur le raccourci : la barre des taches et la
//! fenetre montrent celle, generique, d'un programme quelconque. Sans
//! l'identite, le Gestionnaire des taches et la liste des applications disent
//! « vigie.exe » au lieu de « Vigie ». Un echec de compilation de la ressource
//! n'est pas fatal — on perd l'icone et le nom, pas le programme, et le message
//! le dit.
fn main() {
    println!("cargo:rerun-if-changed=assets/vigie.ico");
    #[cfg(windows)]
    {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("assets/vigie.ico");
        r.set("FileDescription", "Vigie");
        r.set("ProductName", "Vigie");
        r.set("CompanyName", "Stone");
        r.set("LegalCopyright", "Stone");
        r.set("OriginalFilename", "Vigie.exe");
        if let Err(e) = r.compile() {
            println!("cargo:warning=icone et identite non embarquees : {e}");
        }
    }
}
