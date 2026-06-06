# TundraCode

IDE agéntico construido con Rust + Tauri. Combina un editor de código tradicional con agentes de IA profundamente integrados al proyecto.

## Características principales

- **Editor de código** con syntax highlighting via tree-sitter
- **Agentes de IA integrados**: Plan, Build y Ask
- **Cliente LSP nativo** en Rust (sin dependencias de VSCode)
- **Workflow auditado**: Plan → Review → Build con diffs inline
- **Modelos locales y remotos**: Ollama, OpenAI, Anthropic, OpenRouter, etc.
- **Tool Calling**: sistema de herramientas auditables para operaciones seguras
- **Integración Git**: stage, commit, indicadores de estado
- **Dark mode único**: sin toggle, sin emojis, iconografía Lucide

## Stack técnico

| Capa | Tecnología |
|------|-----------|
| Backend / core | Rust |
| Frontend / shell | Tauri v1 |
| Parsing / análisis | tree-sitter |
| LSP client | Rust nativo |

## Estructura del proyecto

```
tundracode/
├── Cargo.toml              # Workspace Rust
├── src-tauri/              # Aplicación Tauri (backend + shell)
│   ├── src/main.rs         # Entry point con comandos Tauri
│   ├── src/lib.rs          # Integración de crates internos
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                    # Frontend (HTML/CSS/JS)
│   ├── index.html          # Shell UI principal
│   ├── main.js             # Lógica del frontend
│   └── styles/main.css     # Estilos dark mode
├── crates/                 # Crates internos del workspace
│   ├── tundracode-core/    # Lógica principal (workspace, archivos, editor)
│   ├── tundracode-tools/   # Sistema de Tool Calling
│   ├── tundracode-agents/  # Agentes Plan, Build, Ask
│   ├── tundracode-models/  # Integración con providers de IA
│   ├── tundracode-lsp/     # Cliente LSP nativo
│   ├── tundracode-git/     # Operaciones Git
│   ├── tundracode-config/  # Configuración y settings
│   └── tundracode-security/ # Keyring y sandbox
├── packaging/              # Scripts de empaquetado
│   ├── flatpak/            # Manifiesto Flatpak
│   └── targz/              # Script .tar.gz
└── .github/workflows/      # CI/CD
```

## Requisitos

### Sistema (Linux)

```bash
# Debian/Ubuntu
sudo apt-get install libwebkit2gtk-4.0-dev libgtk-3-dev libsoup2.4-dev \
    libjavascriptcoregtk-4.0-dev libssl-dev pkg-config

# Fedora
sudo dnf install webkit2gtk3-devel gtk3-devel libsoup-devel \
    javascriptcoregtk4.0-devel openssl-devel pkgconfig

# Arch
sudo pacman -S webkit2gtk gtk3 libsoup js102 openssl pkgconf
```

### Rust

```bash
rustup update
rustup target add x86_64-unknown-linux-gnu
```

### Tauri CLI (opcional)

```bash
cargo install tauri-cli
```

## Desarrollo

```bash
# Clonar
git clone https://github.com/tundracode/tundracode
cd tundracode

# Verificar compilación del workspace (sin Tauri shell)
cargo check --all

# Compilar solo los crates internos
cargo build --workspace --exclude src-tauri

# Ejecutar la aplicación completa (requiere dependencias de sistema)
cargo tauri dev
# o
cd src-tauri && cargo run

# Tests
cargo test --all
```

## Distribución

### Flatpak (recomendado)

```bash
cd packaging/flatpak
flatpak-builder --repo=repo build-dir com.tundracode.dev.yml
flatpak build-bundle repo tundracode.flatpak com.tundracode.dev
```

### .tar.gz (binario autónomo)

```bash
cd packaging/targz
./build.sh
```

## Licencia

MIT OR Apache-2.0

## Estado

En desarrollo activo. Ver `tundracode_spec_v2.md` para la especificación completa.
