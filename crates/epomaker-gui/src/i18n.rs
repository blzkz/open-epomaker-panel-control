//! Minimal translation layer: UI strings are written in English and looked up here.

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Lang {
    En,
    Es,
}

impl Lang {
    pub fn detect() -> Lang {
        let vars = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"];
        for v in vars {
            if let Ok(val) = std::env::var(v) {
                if !val.is_empty() {
                    return if val.to_lowercase().starts_with("es") { Lang::Es } else { Lang::En };
                }
            }
        }
        #[cfg(windows)]
        {
            // GetUserDefaultLocaleName is not worth a dependency; default to Spanish
            // only when the environment says so.
        }
        Lang::En
    }
}

const ES: &[(&str, &str)] = &[
    ("Keyboard", "Teclado"),
    ("Connect", "Conectar"),
    ("Disconnect", "Desconectar"),
    ("Refresh", "Actualizar"),
    ("Demo keyboard (no hardware)", "Teclado de demostración (sin hardware)"),
    ("No keyboard connected", "Ningún teclado conectado"),
    ("Searching…", "Buscando…"),
    ("Working…", "Trabajando…"),
    ("Lighting", "Iluminación"),
    ("Key mapping", "Asignación de teclas"),
    ("Per-key colors", "Colores por tecla"),
    ("Macros", "Macros"),
    ("Settings", "Ajustes"),
    ("Backup", "Copia de seguridad"),
    ("Effect", "Efecto"),
    ("Speed", "Velocidad"),
    ("Brightness", "Brillo"),
    ("Direction / variant", "Dirección / variante"),
    ("Color", "Color"),
    ("Rainbow colors", "Colores arcoíris"),
    ("Apply", "Aplicar"),
    ("Apply automatically", "Aplicar automáticamente"),
    ("Read from keyboard", "Leer del teclado"),
    ("Layer", "Capa"),
    ("Profile", "Perfil"),
    ("Fn layer", "Capa Fn"),
    ("Select a key on the keyboard.", "Selecciona una tecla en el teclado."),
    ("Action", "Acción"),
    ("Factory default", "Valor de fábrica"),
    ("Disabled", "Desactivada"),
    ("Key", "Tecla"),
    ("Mouse", "Ratón"),
    ("Function", "Función"),
    ("Macro", "Macro"),
    ("Raw", "Bruto"),
    ("Modifier", "Modificador"),
    ("Second key", "Segunda tecla"),
    ("Press a key…", "Pulsa una tecla…"),
    ("Capture", "Capturar"),
    ("Slot", "Ranura"),
    ("Trigger", "Disparo"),
    ("Reset layer to factory", "Restablecer capa de fábrica"),
    ("Current", "Actual"),
    ("Brush", "Pincel"),
    ("Fill all keys", "Rellenar todas"),
    ("Write all to keyboard", "Escribir todo al teclado"),
    ("Paint live", "Pintar en directo"),
    ("Activate custom per-key effect", "Activar efecto personalizado por tecla"),
    ("Click or drag over keys to paint them.", "Haz clic o arrastra sobre las teclas para pintarlas."),
    ("Repeat count", "Repeticiones"),
    ("Events", "Eventos"),
    ("Record", "Grabar"),
    ("Stop recording", "Detener grabación"),
    ("Recording: type on this window…", "Grabando: escribe en esta ventana…"),
    ("Add key press", "Añadir pulsación"),
    ("Add delay", "Añadir pausa"),
    ("Clear", "Vaciar"),
    ("Write to keyboard", "Escribir al teclado"),
    ("in use", "en uso"),
    ("Bytes used", "Bytes usados"),
    ("Macro too long", "Macro demasiado larga"),
    ("Active profile", "Perfil activo"),
    ("Polling rate", "Frecuencia de sondeo"),
    ("Options for this profile", "Opciones de este perfil"),
    ("Disable Windows key", "Desactivar tecla Windows"),
    ("Mac mode", "Modo Mac"),
    ("Swap WASD and arrows", "Intercambiar WASD y flechas"),
    ("Backlight off", "Retroiluminación apagada"),
    ("Side LEDs off", "LEDs laterales apagados"),
    ("Gaming mode", "Modo juego"),
    ("Lock keyboard", "Bloquear teclado"),
    ("Swap F-keys and media keys", "Intercambiar teclas F y multimedia"),
    ("Power saving", "Ahorro de energía"),
    ("Debounce (ms)", "Antirrebote (ms)"),
    ("Sleep timers (minutes)", "Temporizadores de reposo (minutos)"),
    ("Bluetooth: light off", "Bluetooth: apagar luz"),
    ("Bluetooth: deep sleep", "Bluetooth: reposo profundo"),
    ("2.4 GHz: light off", "2,4 GHz: apagar luz"),
    ("2.4 GHz: deep sleep", "2,4 GHz: reposo profundo"),
    ("Automatic OS detection", "Detección automática del sistema"),
    ("Factory reset…", "Restablecer de fábrica…"),
    (
        "This erases every profile, key mapping, macro and lighting setting. Continue?",
        "Esto borra todos los perfiles, asignaciones, macros e iluminación. ¿Continuar?",
    ),
    ("Cancel", "Cancelar"),
    ("Reset", "Restablecer"),
    ("Device", "Dispositivo"),
    ("Firmware", "Firmware"),
    ("Battery", "Batería"),
    (
        "Save the whole configuration of the keyboard to a file, or load one.",
        "Guarda toda la configuración del teclado en un archivo, o carga una.",
    ),
    ("File", "Archivo"),
    ("Save backup", "Guardar copia"),
    ("Load backup", "Cargar copia"),
    ("Saved", "Guardado"),
    ("Restored", "Restaurado"),
    ("Language", "Idioma"),
    ("never", "nunca"),
    ("Done", "Hecho"),
    ("Error", "Error"),
    (
        "Experimental: this software is not affiliated with EPOMAKER.",
        "Experimental: este software no está afiliado a EPOMAKER.",
    ),
    (
        "The effect needs the host to stream audio/screen data; the keyboard will stay dark without it.",
        "Este efecto necesita que el PC envíe audio/pantalla; sin eso el teclado no mostrará nada.",
    ),
    ("Move", "Mover"),
    ("Up", "Arriba"),
    ("Down", "Abajo"),
    ("Delete", "Borrar"),
    ("pressed", "pulsada"),
    ("released", "soltada"),
    ("Delay", "Pausa"),
    ("Mouse move", "Mover ratón"),
    ("Add mouse click", "Añadir clic"),
    ("discharging", "descargando"),
    ("charging", "cargando"),
    ("full", "llena"),
    // ---- top bar, preferences and feedback
    ("demo, no hardware", "demo, sin hardware"),
    ("Look for keyboards again", "Volver a buscar teclados"),
    ("Preferences", "Preferencias"),
    ("Theme", "Tema"),
    ("Follow system", "Según el sistema"),
    ("Light", "Claro"),
    ("Dark", "Oscuro"),
    ("Interface size", "Tamaño de la interfaz"),
    ("Connected", "Conectado"),
    ("done", "hecho"),
    ("Close", "Cerrar"),
    // ---- job names (progress, toasts and errors)
    ("Set lighting", "Aplicar iluminación"),
    ("Read lighting", "Leer iluminación"),
    ("Set key", "Asignar tecla"),
    ("Reset layer", "Restablecer capa"),
    ("Read layer", "Leer capa"),
    ("Write colors", "Escribir colores"),
    ("Read colors", "Leer colores"),
    ("Paint key", "Pintar tecla"),
    ("Write macro", "Escribir macro"),
    ("Read macro", "Leer macro"),
    ("Set profile", "Cambiar perfil"),
    ("Set polling rate", "Cambiar frecuencia de sondeo"),
    ("Set options", "Cambiar opciones"),
    ("Set debounce", "Cambiar antirrebote"),
    ("Set auto OS", "Cambiar detección del sistema"),
    ("Set sleep timers", "Cambiar temporizadores de reposo"),
    ("Factory reset", "Restablecer de fábrica"),
    ("Read battery", "Leer batería"),
    ("Demo remap", "Reasignación de demostración"),
    // ---- lighting effects and their options
    ("Off", "Apagado"),
    ("Static", "Fijo"),
    ("Breathing", "Respiración"),
    ("Neon (spectrum)", "Neón (espectro)"),
    ("Wave", "Ola"),
    ("Ripple", "Onda expansiva"),
    ("Raindrop", "Gotas de lluvia"),
    ("Snake", "Serpiente"),
    ("Reactive (light on press)", "Reactivo (se enciende al pulsar)"),
    ("Converge", "Convergencia"),
    ("Sine wave", "Onda senoidal"),
    ("Kaleidoscope", "Caleidoscopio"),
    ("Line wave", "Línea"),
    ("Custom per-key", "Personalizado por tecla"),
    ("Laser", "Láser"),
    ("Circle wave", "Onda circular"),
    ("Dazzling", "Destellos"),
    ("Rain down", "Lluvia"),
    ("Meteor", "Meteoro"),
    ("Reactive (light off on press)", "Reactivo (se apaga al pulsar)"),
    ("Music follow 1", "Música 1"),
    ("Screen color", "Color de pantalla"),
    ("Music follow 2", "Música 2"),
    ("Train", "Tren"),
    ("Fireworks", "Fuegos artificiales"),
    ("User color", "Color de usuario"),
    ("Single color", "Un color"),
    ("Random colors", "Colores aleatorios"),
    ("Left", "Izquierda"),
    ("Right", "Derecha"),
    ("Whole keyboard", "Todo el teclado"),
    ("From each key", "Desde cada tecla"),
    ("Row by row", "Fila a fila"),
    ("Back and forth", "Ida y vuelta"),
    ("Lit, off on press", "Encendido, se apaga al pulsar"),
    ("Dark, lit on press", "Apagado, se enciende al pulsar"),
    ("Outwards", "Hacia fuera"),
    ("Inwards", "Hacia dentro"),
    ("Counter-clockwise", "Antihorario"),
    ("Clockwise", "Horario"),
    ("Bars", "Barras"),
    ("Split", "Separadas"),
    ("Mirrored", "Reflejadas"),
    ("Not applied yet", "Aún no aplicado"),
    ("Paint the keys on the \"Per-key colors\" tab.", "Pinta las teclas en la pestaña \"Colores por tecla\"."),
    // ---- key actions
    ("New action:", "Nueva acción:"),
    ("now", "ahora"),
    ("None", "Ninguno"),
    ("Search", "Buscar"),
    ("key, function…", "tecla, función…"),
    ("Nothing found", "Sin resultados"),
    ("No key", "Ninguna tecla"),
    ("Use", "Usar"),
    ("Advanced: raw matrix entry", "Avanzado: valor bruto de la matriz"),
    ("Press a key on your keyboard to pick it", "Pulsa una tecla de tu teclado para elegirla"),
    ("The key does what is printed on it.", "La tecla hace lo que tiene impreso."),
    ("The key does nothing.", "La tecla no hace nada."),
    ("Letters & numbers", "Letras y números"),
    ("Symbols", "Símbolos"),
    ("F-keys", "Teclas F"),
    ("Navigation & editing", "Navegación y edición"),
    ("Modifiers", "Modificadores"),
    ("Numpad", "Teclado numérico"),
    ("Media & functions", "Multimedia y funciones"),
    ("Layers", "Capas"),
    ("Mouse left", "Clic izquierdo"),
    ("Mouse right", "Clic derecho"),
    ("Mouse middle", "Clic central"),
    ("Mouse back", "Ratón: atrás"),
    ("Mouse forward", "Ratón: adelante"),
    ("Wheel forward", "Rueda hacia delante"),
    ("Wheel back", "Rueda hacia atrás"),
    ("Wheel left", "Rueda a la izquierda"),
    ("Wheel right", "Rueda a la derecha"),
    ("Scroll up", "Desplazar arriba"),
    ("Scroll down", "Desplazar abajo"),
    ("Move left", "Mover a la izquierda"),
    ("Move right", "Mover a la derecha"),
    ("Move up", "Mover arriba"),
    ("Move down", "Mover abajo"),
    ("Repeat N times", "Repetir N veces"),
    ("Toggle loop", "Bucle activar/desactivar"),
    ("Loop while held", "Bucle mientras se mantiene"),
    ("Previous track", "Pista anterior"),
    ("Next track", "Pista siguiente"),
    ("Stop", "Detener"),
    ("Play/Pause", "Reproducir/Pausa"),
    ("Media player", "Reproductor multimedia"),
    ("Mute", "Silenciar"),
    ("Volume down", "Bajar volumen"),
    ("Volume up", "Subir volumen"),
    ("Calculator", "Calculadora"),
    ("Mail", "Correo"),
    ("My computer", "Mi equipo"),
    ("Web search", "Buscar en la web"),
    ("Web home", "Página de inicio"),
    ("Web back", "Web: atrás"),
    ("Web refresh", "Web: recargar"),
    ("Screen brightness down", "Bajar brillo de pantalla"),
    ("Screen brightness up", "Subir brillo de pantalla"),
    ("Fn", "Fn"),
    ("Right Fn", "Fn derecha"),
    ("Fn lock", "Bloqueo de Fn"),
    ("Siri", "Siri"),
    ("Switch input method", "Cambiar método de entrada"),
    ("Zoom out", "Alejar"),
    ("Zoom in", "Acercar"),
    ("Lock screen", "Bloquear pantalla"),
    ("Knob: volume/backlight", "Rueda: volumen/retroiluminación"),
    ("Knob: volume/backlight 2", "Rueda: volumen/retroiluminación 2"),
    ("Knob: volume/backlight 3", "Rueda: volumen/retroiluminación 3"),
    ("Rapid fire", "Disparo rápido"),
    // ---- per-key colors
    ("Custom picture", "Imagen personalizada"),
    ("(shown on the keyboard)", "(se muestra en el teclado)"),
    ("Show this picture on the keyboard", "Mostrar esta imagen en el teclado"),
    (
        "\"Write all\" sends the whole picture at once. The keyboard's bulk write always goes to picture 1, like in the vendor app.",
        "\"Escribir todo\" envía la imagen completa de una vez. La escritura en bloque del teclado siempre va a la imagen 1, como en la aplicación del fabricante.",
    ),
    (
        "Pictures 2 and 3 cannot be read back and the bulk write only reaches picture 1, so they are written key by key (slower).",
        "Las imágenes 2 y 3 no se pueden leer del teclado y la escritura en bloque solo llega a la imagen 1, así que se escriben tecla a tecla (más lento).",
    ),
    ("Tool", "Herramienta"),
    ("Paint", "Pintar"),
    ("Select", "Seleccionar"),
    ("Select rows", "Seleccionar filas"),
    (
        "Click keys, or drag a box around them. Hold Shift to add to the selection.",
        "Haz clic en las teclas o arrastra un recuadro a su alrededor. Mantén Mayús para añadir a la selección.",
    ),
    ("Click a key to select its row.", "Haz clic en una tecla para seleccionar su fila."),
    ("(recent colors appear here)", "(aquí aparecen los colores recientes)"),
    ("Send every change to the keyboard right away", "Envía cada cambio al teclado al momento"),
    ("No keys selected: fills apply to the whole keyboard.", "Sin teclas seleccionadas: los rellenos afectan a todo el teclado."),
    ("keys selected", "teclas seleccionadas"),
    ("Select all", "Seleccionar todo"),
    ("Clear selection", "Quitar selección"),
    ("Fill with brush", "Rellenar con el pincel"),
    ("Gradient from brush to", "Degradado del pincel a"),
    ("left to right", "de izquierda a derecha"),
    ("top to bottom", "de arriba abajo"),
    ("Apply gradient", "Aplicar degradado"),
    ("Write all to keyboard (key by key)", "Escribir todo al teclado (tecla a tecla)"),
    ("Only picture 1 can be read back.", "Solo se puede leer la imagen 1."),
    // ---- udev helper (Linux)
    ("Permission needed to access the keyboard", "Se necesita permiso para acceder al teclado"),
    (
        "Linux only lets root open the keyboard's configuration interface until a udev rule grants access to the logged-in user. The rule only matches EPOMAKER / RongYuan devices (USB vendor 3151).",
        "Linux solo permite a root abrir la interfaz de configuración del teclado hasta que una regla de udev da acceso al usuario con la sesión iniciada. La regla solo afecta a dispositivos EPOMAKER / RongYuan (fabricante USB 3151).",
    ),
    ("It will be installed as", "Se instalará como"),
    ("Install rule (asks for your password)", "Instalar la regla (pide tu contraseña)"),
    ("Copy commands", "Copiar comandos"),
    ("To run them yourself in a terminal", "Para ejecutarlos tú mismo en una terminal"),
    ("Could not install the rule", "No se pudo instalar la regla"),
    ("Rule installed. Reconnecting…", "Regla instalada. Reconectando…"),
];

pub fn tr(lang: Lang, s: &str) -> &str {
    match lang {
        Lang::En => s,
        Lang::Es => ES.iter().find(|(en, _)| *en == s).map(|(_, es)| *es).unwrap_or(s),
    }
}

/// English display text for a light effect option name from the device database.
pub fn option_label(raw: &str) -> &str {
    match raw {
        "Default" => "Single color",
        "Random" => "Random colors",
        "right" => "Right",
        "left" => "Left",
        "down" => "Down",
        "up" => "Up",
        "full" => "Whole keyboard",
        "single" => "From each key",
        "z" => "Row by row",
        "return" => "Back and forth",
        "onToOff" => "Lit, off on press",
        "offToOn" => "Dark, lit on press",
        "out" => "Outwards",
        "in" => "Inwards",
        "anti-clockwise" => "Counter-clockwise",
        "clockwise" => "Clockwise",
        "upright" => "Bars",
        "separate" => "Split",
        "intersect" => "Mirrored",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epomaker_core::devices::Database;
    use epomaker_core::keymap::{MacroMode, MouseAction, SpecialFn};
    use epomaker_core::lighting::LightMode;

    /// Sources whose UI strings go through `tr`.
    const SOURCES: &[(&str, &str)] = &[
        ("app.rs", include_str!("app.rs")),
        ("app/lighting.rs", include_str!("app/lighting.rs")),
        ("app/colors.rs", include_str!("app/colors.rs")),
        ("editors.rs", include_str!("editors.rs")),
        ("picker.rs", include_str!("picker.rs")),
        ("udev.rs", include_str!("udev.rs")),
    ];

    /// Reads the Rust string literal starting right after its opening quote.
    fn literal(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('\n') => {
                        while chars.peek().is_some_and(|c| c.is_whitespace()) {
                            chars.next();
                        }
                    }
                    Some(c) => out.push(c),
                    None => break,
                },
                c => out.push(c),
            }
        }
        out
    }

    /// Strings passed to the translation functions, plus labels returned by `=> "…"` arms
    /// and job labels.
    fn ui_strings(name: &str, src: &str) -> Vec<String> {
        let mut needles = vec!["t(", "tr(lang, ", "tr(self.lang, ", "run(", "run_notify(", "label: "];
        needles.extend(["quiet(", "notify(", "blocking("]);
        if name != "editors.rs" {
            needles.push("=> ");
        }
        let mut found = vec![];
        for n in needles {
            for (pos, _) in src.match_indices(n) {
                let before = src[..pos].chars().next_back().unwrap_or(' ');
                if n.starts_with('t') && (before.is_alphanumeric() || before == '_') {
                    continue;
                }
                let rest = src[pos + n.len()..].trim_start();
                if let Some(lit) = rest.strip_prefix('"') {
                    found.push(literal(lit));
                }
            }
        }
        found
    }

    #[test]
    fn spanish_covers_the_ui() {
        let mut wanted: Vec<String> = SOURCES.iter().flat_map(|(n, s)| ui_strings(n, s)).collect();
        wanted.extend(LightMode::ALL.iter().map(|m| m.label().to_string()));
        wanted.extend(MouseAction::ALL.iter().map(|m| m.label().to_string()));
        wanted.extend(MacroMode::ALL.iter().map(|m| m.label().to_string()));
        wanted.extend(LightMode::ALL.iter().flat_map(|m| m.options()).map(|o| option_label(o).to_string()));
        // modifier key names stay as printed on the keycaps, except "None"
        wanted.push(epomaker_core::keymap::Modifier::None.label().to_string());
        wanted.extend(
            SpecialFn::ALL.iter().map(|f| f.label()).filter(|l| l.chars().any(char::is_alphabetic)).map(String::from),
        );
        for model in Database::get().models.values() {
            for caps in &model.light_modes {
                wanted.extend(caps.options.iter().map(|o| option_label(o).to_string()));
            }
        }
        wanted.retain(|s| s.chars().any(char::is_alphabetic) && !s.contains('{'));
        wanted.sort();
        wanted.dedup();
        let missing: Vec<_> = wanted.iter().filter(|s| !ES.iter().any(|(en, _)| en == s)).collect();
        assert!(missing.is_empty(), "missing Spanish translations: {missing:#?}");
    }

    #[test]
    fn no_duplicate_entries() {
        for (i, (en, _)) in ES.iter().enumerate() {
            assert!(!ES[i + 1..].iter().any(|(e, _)| e == en), "duplicate entry {en:?}");
        }
    }
}
