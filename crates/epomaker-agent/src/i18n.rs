//! Tray texts in English or Spanish, following the system language.

use std::sync::OnceLock;

fn spanish() -> bool {
    static ES: OnceLock<bool> = OnceLock::new();
    *ES.get_or_init(|| {
        ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
            .iter()
            .filter_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
            .next()
            .is_some_and(|l| l.to_lowercase().starts_with("es"))
    })
}

const ES: &[(&str, &str)] = &[
    ("No keyboard connected", "Ningún teclado conectado"),
    ("Paused while the control panel is open", "En pausa mientras el panel está abierto"),
    ("Profile", "Perfil"),
    ("Live effect", "Efecto en vivo"),
    ("Off", "Desactivado"),
    ("Music follow 1", "Música 1"),
    ("Music follow 2", "Música 2"),
    ("Screen color", "Color de pantalla"),
    ("CPU load", "Carga de CPU"),
    ("Per-application profiles", "Perfiles por aplicación"),
    ("Edit rules…", "Editar reglas…"),
    ("Open control panel", "Abrir panel de control"),
    ("Start with the system", "Iniciar con el sistema"),
    ("Quit", "Salir"),
    ("Battery", "Batería"),
    ("Active app", "Aplicación activa"),
    ("Keyboard battery low", "Batería del teclado baja"),
    ("not available on this desktop", "no disponible en este escritorio"),
];

pub fn t(s: &'static str) -> &'static str {
    if spanish() {
        ES.iter().find(|(en, _)| *en == s).map(|(_, es)| *es).unwrap_or(s)
    } else {
        s
    }
}
