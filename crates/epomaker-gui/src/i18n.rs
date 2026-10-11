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
    ("Up", "Subir"),
    ("Down", "Bajar"),
    ("Delete", "Borrar"),
    ("pressed", "pulsada"),
    ("released", "soltada"),
    ("Delay", "Pausa"),
    ("Mouse move", "Mover ratón"),
    ("Add mouse click", "Añadir clic"),
    ("discharging", "descargando"),
    ("charging", "cargando"),
    ("full", "llena"),
    ("Diagnostics", "Diagnóstico"),
    ("Hardware self-test", "Autotest del hardware"),
    (
        "Checks that this app understands your keyboard. It only reads, except for three small changes that are undone right away: the backlight brightness, one rarely used key on profile 3 and an unused macro slot. The original state is restored even if a step fails. Then copy the report into a GitHub issue.",
        "Comprueba que esta aplicación entiende tu teclado. Solo lee, salvo tres pequeños cambios que se deshacen al momento: el brillo de la retroiluminación, una tecla poco usada del perfil 3 y una ranura de macro libre. El estado original se restaura aunque falle un paso. Después copia el informe en una incidencia de GitHub.",
    ),
    ("Verify every write (read it back)", "Verificar cada escritura (releer el valor)"),
    (
        "Every change made in this app is read back from the keyboard; a mismatch is shown as an error.",
        "Cada cambio hecho en esta aplicación se vuelve a leer del teclado; si no coincide se muestra un error.",
    ),
    ("Run self-test", "Ejecutar autotest"),
    ("Copy report", "Copiar informe"),
    ("Report copied to the clipboard", "Informe copiado al portapapeles"),
    ("Report preview", "Vista previa del informe"),
    ("passed", "correcto"),
    ("FAILED", "FALLO"),
    ("skipped", "omitido"),
    ("pending", "pendiente"),
    ("running…", "en curso…"),
    ("restored", "restaurado"),
    ("NOT restored", "NO restaurado"),
    ("Identify keyboard", "Identificar el teclado"),
    ("Read every setting", "Leer todos los ajustes"),
    ("Change brightness and restore", "Cambiar el brillo y restaurarlo"),
    ("Remap a key and restore", "Reasignar una tecla y restaurarla"),
    ("Write a macro and erase it", "Escribir una macro y borrarla"),
    ("Read-only probe", "Sondeo de solo lectura"),
    (
        "No supported keyboard is connected. The probe lists the USB interfaces of EPOMAKER keyboards and sends only read commands. Share its output in a GitHub issue if your keyboard is not detected.",
        "No hay ningún teclado compatible conectado. El sondeo lista las interfaces USB de los teclados EPOMAKER y solo envía órdenes de lectura. Comparte su salida en una incidencia de GitHub si tu teclado no se detecta.",
    ),
    ("Run probe", "Ejecutar sondeo"),
    ("Copy output", "Copiar salida"),
    ("Output copied to the clipboard", "Salida copiada al portapapeles"),
    ("Keyboard disconnected", "Teclado desconectado"),
    ("It will reconnect automatically when plugged in again.", "Se volverá a conectar automáticamente al enchufarlo de nuevo."),
];

pub fn tr(lang: Lang, s: &str) -> &str {
    match lang {
        Lang::En => s,
        Lang::Es => ES.iter().find(|(en, _)| *en == s).map(|(_, es)| *es).unwrap_or(s),
    }
}
