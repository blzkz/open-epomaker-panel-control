# Open Epomaker Panel

Panel de control de código abierto para teclados **EPOMAKER** en **Linux y Windows**, escrito
en Rust. Sustituye al «EPOMAKER Driver» oficial para los teclados EPOMAKER basados en la
plataforma RongYuan YC500 (ver [Teclados soportados](#teclados-soportados)).

> ⚠️ **Estado: experimental.** El protocolo se ha obtenido por ingeniería inversa del software
> oficial y se comprueba byte a byte contra él (2359 vectores de prueba en 5 modelos), pero **todavía no se
> ha probado con un teclado físico**. Empieza por el comando de solo lectura `epomaker probe`.
> Este proyecto no está afiliado a EPOMAKER.

## Funciones

- Detección del modelo a partir del ID del firmware.
- Iluminación: 19 efectos, con velocidad, brillo, dirección, color y modo arcoíris.
- RGB por tecla (efecto personalizado), pintando con el ratón.
- Reasignación de teclas en los 3 perfiles y en la capa Fn: teclas, combinaciones, ratón, multimedia y macros.
- Macros (50 ranuras): editor, grabación y modos repetir, alternar o mantener.
- Perfiles, frecuencia de sondeo, bloqueo de la tecla Win, modo Mac, intercambio WASD/flechas e intercambio de la capa Fn.
- Antirrebote, temporizadores de reposo inalámbrico y detección automática del sistema.
- Copia de seguridad y restauración completas en JSON, y restablecimiento de fábrica.
- Modo demo con un teclado emulado, sin necesidad de hardware.
- Interfaz gráfica (español o inglés) y herramienta de línea de comandos.

## Teclados soportados

Todos los modelos funcionan por cable (USB) y con el receptor de 2,4 GHz. Cada uno se comprueba
contra vectores de bytes grabados del software oficial; ninguno se ha probado aún con hardware.

| Modelo | ID de firmware (cable) | `--demo-model` |
|---|---|---|
| TH80, TH80 Pro | 1118, 742 | `1118` |
| TH80 SOC | 1091 | `1091` |
| TH80 SE | 886 | `886` |
| TH80 X (sin soporte para la pantalla) | 928 | `928` |
| TH80 UK | 741 | `741` |
| TH96 | 834 | `834` |
| TH68 | 988 | `988` |
| TH-21 (teclado numérico) | 789 | `789` |
| EP84 | 1081 | `1081` |

Se pueden añadir otros modelos YC500 (por ejemplo el RT100, que tiene pantalla) a partir de los
datos del software oficial; ver [docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md#adding-another-yc500-keyboard).

## Descargar

Cada [release en GitHub](https://github.com/blzkz/open-epomaker-panel-control/releases) incluye paquetes listos:

| Sistema | Paquete |
|---|---|
| Windows 10/11 | `open-epomaker-panel-windows-x86_64.zip`: descomprime y ejecuta `epomaker-panel.exe` |
| Debian, Ubuntu, Mint, Pop!_OS | `.deb`: `sudo apt install ./open-epomaker-panel_*_amd64.deb` |
| Fedora, RHEL, openSUSE | `.rpm`: `sudo dnf install ./open-epomaker-panel-*.x86_64.rpm` (o `zypper install`) |
| Arch, Manjaro, EndeavourOS, CachyOS | `.pkg.tar.zst`: `sudo pacman -U open-epomaker-panel-*.pkg.tar.zst` |
| Cualquier Linux | `open-epomaker-panel-x86_64.AppImage` (portable) o el `.tar.gz` con `linux/install.sh` |

Los paquetes deb, rpm y de Arch instalan también la regla udev que da acceso al teclado sin root.
Con el AppImage tienes que instalar tú `packaging/linux/70-epomaker.rules`.

## Compilar

Necesitas Rust 1.88 o superior. En Linux necesitas además las cabeceras de libudev
(`sudo apt install libudev-dev libxkbcommon-dev libgl1-mesa-dev libwayland-dev` en Debian/Ubuntu).

```bash
cargo build --release
packaging/linux/install.sh   # Linux: binarios, regla udev y acceso directo
```

En Windows no hace falta ningún driver. Ejecuta `epomaker-panel.exe` y cierra antes el
software oficial de EPOMAKER.

## Uso rápido

```bash
epomaker probe                       # diagnóstico de solo lectura (empieza aquí)
epomaker info
epomaker light set wave --option left --color ff0080
epomaker keymap set "Caps Lock" disable
epomaker keymap set A key:ctrl+c
epomaker --demo info                 # con el teclado emulado
```

La interfaz gráfica es `epomaker-panel`. Consulta el [README en inglés](README.md) para más
ejemplos y [docs/PROTOCOL.md](docs/PROTOCOL.md) para la descripción del protocolo.

## Licencia

[MIT](LICENSE). Proyecto no afiliado ni respaldado por EPOMAKER.
