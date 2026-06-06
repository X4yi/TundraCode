#!/usr/bin/env python3
"""
strip_comments.py
=================

Elimina comentarios de un proyecto completo, adaptándose al lenguaje de cada
archivo. Usa una state-machine consciente de strings para no eliminar
comentarios que aparezcan dentro de literales.

Lenguajes soportados:
    Rust, JS/TS, HTML/Markdown/XML, CSS/SCSS, TOML, JSON, YAML, Shell,
    Python, C/C++, Java, Kotlin, Go, Ruby, PHP, Swift, SQL, Lua.

Preserva:
    - Strings (con escapes \\, \", \n, ...)
    - Raw strings de Rust: r"..." r#"..."# r##"..."##
    - Byte strings: b"..." br"..."
    - Template literals de JS/TS: `...` con interpolación ${...}
    - Regex literales de JS/TS: /patrón/flags (heurística conservadora)
    - Strings multilínea: tres comillas dobles o tres comillas simples (Python/TOML)
    - Shebang #! en la primera línea de scripts ejecutables
    - Cadenas dentro de comentarios: /* "no es string" */
    - Comentarios con prefijo ! (importantes / TODO / FIXME):
        // !texto   /* !texto */   # !texto   <!-- !texto -->
        //!texto    (Rust inner doc)   ///!texto (Rust outer doc)
        =begin !texto (Ruby)

Por defecto NO modifica archivos (modo dry-run). Usa --write para aplicar.

Uso:
    python scripts/strip_comments.py
    python scripts/strip_comments.py --write
    python scripts/strip_comments.py src/ src-tauri/ --ext rs,js,ts
    python scripts/strip_comments.py --write --verbose
    python scripts/strip_comments.py --include-lock --no-exclude
    python scripts/strip_comments.py --unsafe-html  # procesar HTML inseguros
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

# ════════════════════════════════════════════════════════════
#  Configuración por defecto
# ════════════════════════════════════════════════════════════

EXCLUDED_DIRS: set[str] = {
    "target", "node_modules", ".git", "dist", "build",
    ".opencode", "gen", "icons", ".venv", "venv", "__pycache__",
    ".idea", ".vscode", "out", ".next", ".cache", "coverage",
    ".mypy_cache", ".pytest_cache", ".tox", "vendor", "third_party",
}

EXCLUDED_FILE_NAMES: set[str] = {
    "Cargo.lock", "package-lock.json", "yarn.lock", "pnpm-lock.yaml",
    "poetry.lock", "Pipfile.lock", "composer.lock", "Gemfile.lock",
}

# Patrones de archivos minificados a excluir
MINIFIED_PATTERNS: tuple[str, ...] = (
    ".min.js", ".min.css", ".min.mjs",
    "-min.js", "-min.css",
    ".bundle.js", ".bundle.css",
    ".prod.js",
)

BINARY_EXTS: set[str] = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".tiff", ".ico", ".icns",
    ".woff", ".woff2", ".ttf", ".otf", ".eot",
    ".pdf", ".zip", ".tar", ".gz", ".tgz", ".bz2", ".xz", ".7z", ".rar",
    ".mp3", ".mp4", ".mov", ".avi", ".mkv", ".webm", ".ogg", ".flac",
    ".exe", ".dll", ".so", ".dylib", ".a", ".o", ".rlib", ".lib",
    ".class", ".jar", ".war", ".pyc", ".pyo", ".pyd",
    ".db", ".sqlite", ".sqlite3", ".bin",
}

EXT_LANG: dict[str, str] = {
    ".rs": "rust",
    ".js": "js", ".jsx": "js", ".mjs": "js", ".cjs": "js",
    ".ts": "ts", ".tsx": "ts",
    ".html": "html", ".htm": "html",
    ".md": "md", ".markdown": "md",
    ".svg": "xml", ".xml": "xml",
    ".css": "css", ".scss": "css", ".sass": "css", ".less": "css",
    ".toml": "toml",
    ".json": "json",
    ".yml": "yaml", ".yaml": "yaml",
    ".sh": "shell", ".bash": "shell", ".zsh": "shell", ".ksh": "shell",
    ".py": "py", ".pyi": "py",
    ".c": "c", ".h": "c",
    ".cpp": "cpp", ".cc": "cpp", ".cxx": "cpp", ".hpp": "cpp", ".hxx": "cpp",
    ".java": "java",
    ".kt": "kotlin", ".kts": "kotlin",
    ".go": "go",
    ".rb": "ruby",
    ".php": "php",
    ".swift": "swift",
    ".sql": "sql",
    ".lua": "lua",
}

# Caracteres que, si aparecen como primer carácter no-espacio tras un marcador
# de apertura de comentario, indican que el comentario debe preservarse.
# Ejemplos: "// !importante", "# !TODO", "/* !FIXME */", "<!-- !aviso -->"
PRESERVE_PREFIXES: set[str] = {"!"}

# ════════════════════════════════════════════════════════════
#  State machines por familia de lenguajes
# ════════════════════════════════════════════════════════════

def _is_id_char(c: str) -> bool:
    return c.isalnum() or c == "_" or c == "$"


def _peek_preserve_prefix(src: str, i: int, n: int) -> bool:
    """Devuelve True si el primer carácter no-espacio desde i es un prefijo preservado.

    Usado para detectar comentarios que NO deben eliminarse, p.ej. "// !importante".
    """
    j = i
    while j < n and src[j] in " \t":
        j += 1
    if j < n and src[j] in PRESERVE_PREFIXES:
        return True
    return False


def _strip_common(out: list[str], i: int, n: int, c: str, c2: str,
                  state: str, block_depth: int, removed: int):
    """Devuelve (nuevo_state, nuevo_block_depth, nuevo_removed, i_avanzado)."""
    if state == "line":
        if c == "\n":
            out.append("\n")
            return "normal", 0, removed, i + 1
        return "line", 0, removed, i + 1
    if state == "block":
        if c == "/" and c2 == "*":
            return "block", block_depth + 1, removed, i + 2
        if c == "*" and c2 == "/":
            nd = block_depth - 1
            return ("normal", 0, removed, i + 2) if nd == 0 else ("block", nd, removed, i + 2)
        return "block", block_depth, removed, i + 1
    return state, block_depth, removed, i


def strip_rust(src: str) -> tuple[str, int]:
    """Rust: // línea, /* */ bloque anidable, "..."/'...', raw strings r".."/r#".."#.

    Preserva comentarios con prefijo !: // !, /* !, //! y ///!.
    """
    out: list[str] = []
    i, n = 0, len(src)
    state = "normal"
    block_depth = 0
    raw_hashes = 0
    removed = 0
    while i < n:
        c = src[i]
        c2 = src[i+1] if i+1 < n else ""
        c3 = src[i+2] if i+2 < n else ""
        c4 = src[i+3] if i+3 < n else ""
        if state == "normal":
            # Prefijos br, b
            if c in "bB" and c2 == "r" and c3 == '"':
                out.append(src[i:i+3]); i += 3; state = "raw"; raw_hashes = 0; continue
            if c in "bB" and c2 == "r" and c3 == "#":
                j = i + 3; h = 0
                while j < n and src[j] == "#":
                    h += 1; j += 1
                if j < n and src[j] == '"':
                    out.append(src[i:j+1]); i = j + 1; state = "raw"; raw_hashes = h; continue
            if c in "bB" and c2 == '"':
                out.append(src[i:i+2]); i += 2; state = "string"; continue
            # Raw string r"..." o r#"..."#
            if c == "r" and c2 == '"':
                out.append('r"'); i += 2; state = "raw"; raw_hashes = 0; continue
            if c == "r" and c2 == "#":
                j = i + 2; h = 0
                while j < n and src[j] == "#":
                    h += 1; j += 1
                if j < n and src[j] == '"':
                    out.append(src[i:j+1]); i = j + 1; state = "raw"; raw_hashes = h; continue
                # Si no es raw string, trata 'r' como identificador
            # Comentarios
            if c == "/" and c2 == "/":
                # Distinguir /// (outer doc), //! (inner doc) y // (regular).
                if c3 == "/":
                    # /// outer doc; ///! se preserva
                    if c4 == "!":
                        out.append(c); out.append(c2); out.append(c3); out.append(c4); i += 4
                        continue
                    i += 3; state = "line"; removed += 1; continue
                if c3 == "!":
                    # //! inner doc; //! + ! se preserva
                    if _peek_preserve_prefix(src, i + 3, n):
                        out.append(c); out.append(c2); out.append(c3); i += 3
                        continue
                    i += 3; state = "line"; removed += 1; continue
                # // regular; // ! se preserva
                if _peek_preserve_prefix(src, i + 2, n):
                    out.append(c); out.append(c2); i += 2
                    continue
                i += 2; state = "line"; removed += 1; continue
            if c == "/" and c2 == "*":
                if _peek_preserve_prefix(src, i + 2, n):
                    out.append(c); out.append(c2); i += 2
                    continue
                i += 2; state = "block"; block_depth = 1; removed += 1; continue
            # Strings/char
            if c == '"':
                out.append(c); i += 1; state = "string"; continue
            if c == "'":
                out.append(c); i += 1; state = "char"; continue
            out.append(c); i += 1
        elif state == "line":
            if c == "\n":
                out.append("\n"); i += 1; state = "normal"
            else:
                i += 1
        elif state == "block":
            if c == "/" and c2 == "*":
                block_depth += 1; i += 2
            elif c == "*" and c2 == "/":
                block_depth -= 1; i += 2
                if block_depth == 0:
                    state = "normal"
            else:
                i += 1
        elif state == "string":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == '"':
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "char":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == "'":
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "raw":
            if c == '"':
                j = i + 1; h = 0
                while j < n and src[j] == "#" and h < raw_hashes:
                    h += 1; j += 1
                if h == raw_hashes:
                    out.append(src[i:j]); i = j; state = "normal"
                else:
                    out.append(c); i += 1
            else:
                out.append(c); i += 1
    return "".join(out), removed


def _prev_meaningful_idx(out: list[str], end: int) -> int:
    """Índice en out del último carácter no-espacio, no-newline."""
    j = end - 1
    while j >= 0 and out[j] in " \t\r\n":
        j -= 1
    return j


def strip_c_like(src: str, has_template: bool, has_regex: bool) -> tuple[str, int]:
    """C-like: // línea, /* */ bloque, "..."/'...', template `...${...}`, regex /.../.

    Preserva comentarios con prefijo !: // ! y /* !.
    """
    out: list[str] = []
    i, n = 0, len(src)
    state = "normal"
    block_depth = 0
    tpl_brace = 0  # anidación de ${} en template
    removed = 0
    # Caracteres tras los cuales '/' probablemente abre un regex
    REGEX_PREV = set("=(,[{!?&|;:+-*/%^~<> \t\n")
    # Caracteres tras los cuales '/' es división
    NOT_REGEX_PREV = set(")\"'`w")
    # Caracteres que en el contexto de template no cuentan para regex
    in_template_expr = False

    def last_meaningful() -> str:
        if not out:
            return " "
        j = len(out) - 1
        while j >= 0 and out[j] in " \t\r\n":
            j -= 1
        if j < 0:
            return " "
        return out[j]

    while i < n:
        c = src[i]
        c2 = src[i+1] if i+1 < n else ""
        c3 = src[i+2] if i+2 < n else ""

        if state == "normal":
            if c == "/" and c2 == "/":
                if _peek_preserve_prefix(src, i + 2, n):
                    out.append(c); out.append(c2); i += 2; continue
                i += 2; state = "line"; removed += 1; continue
            if c == "/" and c2 == "*":
                if _peek_preserve_prefix(src, i + 2, n):
                    out.append(c); out.append(c2); i += 2; continue
                i += 2; state = "block"; block_depth = 1; removed += 1; continue
            if c == '"':
                out.append(c); i += 1; state = "string_d"; continue
            if c == "'":
                out.append(c); i += 1; state = "string_s"; continue
            if has_template and c == "`":
                out.append(c); i += 1; state = "template"; continue
            if has_regex and c == "/":
                # Detectar regex literal
                prev = last_meaningful()
                # Si el anterior es identificador o cierre, NO es regex
                if prev.isalnum() or prev in "_$)]}\"'`":
                    out.append(c); i += 1; continue
                # Heurística adicional: '/' no puede ir seguido de '/' o '*' (sería comentario)
                if c2 in "/*":
                    out.append(c); i += 1; continue
                # Posible regex
                out.append(c); i += 1; state = "regex"; continue
            out.append(c); i += 1
        elif state == "line":
            if c == "\n":
                out.append("\n"); i += 1; state = "normal"
            else:
                i += 1
        elif state == "block":
            if c == "/" and c2 == "*":
                block_depth += 1; i += 2
            elif c == "*" and c2 == "/":
                block_depth -= 1; i += 2
                if block_depth == 0:
                    state = "normal"
            else:
                i += 1
        elif state == "string_d":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == '"':
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "string_s":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == "'":
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "template":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == "`":
                out.append(c); i += 1; state = "normal"
            elif c == "$" and c2 == "{":
                out.append("${"); i += 2; state = "tpl_expr"; tpl_brace = 1
            else:
                out.append(c); i += 1
        elif state == "tpl_expr":
            if c == "{":
                tpl_brace += 1; out.append(c); i += 1
            elif c == "}":
                tpl_brace -= 1
                if tpl_brace == 0:
                    out.append(c); i += 1; state = "template"
                else:
                    out.append(c); i += 1
            elif c == '"':
                out.append(c); i += 1; state = "tpl_str_d"
            elif c == "'":
                out.append(c); i += 1; state = "tpl_str_s"
            elif c == "/" and c2 == "/":
                if _peek_preserve_prefix(src, i + 2, n):
                    out.append(c); out.append(c2); i += 2; continue
                i += 2; state = "tpl_line"; removed += 1
            elif c == "/" and c2 == "*":
                if _peek_preserve_prefix(src, i + 2, n):
                    out.append(c); out.append(c2); i += 2; continue
                i += 2; state = "tpl_block"; block_depth = 1; removed += 1
            else:
                out.append(c); i += 1
        elif state == "tpl_str_d":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == '"':
                out.append(c); i += 1; state = "tpl_expr"
            else:
                out.append(c); i += 1
        elif state == "tpl_str_s":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == "'":
                out.append(c); i += 1; state = "tpl_expr"
            else:
                out.append(c); i += 1
        elif state == "tpl_line":
            if c == "\n":
                out.append("\n"); i += 1; state = "tpl_expr"
            else:
                i += 1
        elif state == "tpl_block":
            if c == "/" and c2 == "*":
                block_depth += 1; i += 2
            elif c == "*" and c2 == "/":
                block_depth -= 1; i += 2
                if block_depth == 0:
                    state = "tpl_expr"
            else:
                i += 1
        elif state == "regex":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == "[":
                out.append(c); i += 1; state = "regex_class"
            elif c == "/":
                out.append(c); i += 1; state = "regex_flags"
            elif c == "\n":
                # Regex no debería cruzar líneas; abortar y tratar / como normal
                state = "normal"
                # No consumir el \n
            else:
                out.append(c); i += 1
        elif state == "regex_class":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == "]":
                out.append(c); i += 1; state = "regex"
            else:
                out.append(c); i += 1
        elif state == "regex_flags":
            if c.isalpha():
                out.append(c); i += 1
            else:
                state = "normal"
                # No incrementar i, reprocesar este carácter
        else:
            out.append(c); i += 1
    return "".join(out), removed


def strip_hash_line(src: str, supports_triple: bool) -> tuple[str, int]:
    """Lenguajes con # como comentario de línea. Soporta strings simples/dobles y triples.

    Preserva el shebang en posición 0 y los comentarios con prefijo !: # !.
    """
    out: list[str] = []
    i, n = 0, len(src)
    state = "normal"
    removed = 0

    while i < n:
        c = src[i]
        c2 = src[i+1] if i+1 < n else ""
        c3 = src[i+2] if i+2 < n else ""

        if state == "normal":
            # Shebang solo en posición 0 del archivo (#!)
            if i == 0 and c == "#" and c2 == "!":
                while i < n and src[i] != "\n":
                    out.append(src[i]); i += 1
                if i < n:
                    out.append("\n"); i += 1
                continue

            # Strings
            if supports_triple and c == '"' and c2 == '"' and c3 == '"':
                out.append('"""'); i += 3; state = "triple_d"; continue
            if supports_triple and c == "'" and c2 == "'" and c3 == "'":
                out.append("'''"); i += 3; state = "triple_s"; continue
            if c == '"':
                out.append(c); i += 1; state = "string_d"; continue
            if c == "'":
                out.append(c); i += 1; state = "string_s"; continue
            # Comentario de línea: con prefijo ! se preserva
            if c == "#":
                if _peek_preserve_prefix(src, i + 1, n):
                    out.append(c); i += 1
                    continue
                i += 1; state = "line"; removed += 1; continue
            out.append(c); i += 1
        elif state == "line":
            if c == "\n":
                out.append("\n"); i += 1; state = "normal"
            else:
                i += 1
        elif state == "string_d":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == '"':
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "string_s":
            if c == "'":
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "triple_d":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == '"' and c2 == '"' and c3 == '"':
                out.append('"""'); i += 3; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "triple_s":
            if c == "'" and c2 == "'" and c3 == "'":
                out.append("'''"); i += 3; state = "normal"
            else:
                out.append(c); i += 1
    return "".join(out), removed


def strip_html_xml(src: str) -> tuple[str, int]:
    """HTML/XML/Markdown: <!-- ... --> (puede multilínea).

    Preserva comentarios con prefijo !: <!-- !.
    """
    out: list[str] = []
    i, n = 0, len(src)
    state = "normal"
    removed = 0
    while i < n:
        c = src[i]
        c2 = src[i+1] if i+1 < n else ""
        c3 = src[i+2] if i+2 < n else ""
        c4 = src[i+3] if i+3 < n else ""
        if state == "normal":
            if c == "<" and c2 == "!" and c3 == "-" and c4 == "-":
                # Comprobar prefijo ! tras <!--
                if _peek_preserve_prefix(src, i + 4, n):
                    out.append(c); out.append(c2); out.append(c3); out.append(c4); i += 4
                    continue
                i += 4; state = "block"; removed += 1; continue
            out.append(c); i += 1
        elif state == "block":
            if c == "-" and c2 == "-" and c3 == ">":
                i += 3; state = "normal"
            elif c == "\n":
                out.append("\n"); i += 1
            else:
                i += 1
    return "".join(out), removed


def strip_ruby(src: str) -> tuple[str, int]:
    """Ruby: # línea, =begin ... =end bloque.

    Preserva comentarios con prefijo !: # ! y =begin !.
    """
    out: list[str] = []
    i, n = 0, len(src)
    state = "normal"
    block_indent = -1
    removed = 0
    while i < n:
        c = src[i]
        c2 = src[i+1] if i+1 < n else ""
        if state == "normal":
            # =begin ... =end
            if c == "=" and src[i:i+6] == "=begin":
                # debe estar al inicio de línea (solo espacios antes)
                j = len(out) - 1
                at_line_start = True
                while j >= 0:
                    if out[j] == "\n":
                        break
                    if out[j] not in " \t":
                        at_line_start = False
                        break
                    j -= 1
                if at_line_start:
                    # Preservar si prefijo !
                    if _peek_preserve_prefix(src, i + 6, n):
                        out.append("=begin"); i += 6
                        continue
                    i += 6
                    # saltar el \n
                    if i < n and src[i] == "\n":
                        i += 1
                    state = "rb_block"
                    removed += 1
                    continue
            if c == "#":
                if _peek_preserve_prefix(src, i + 1, n):
                    out.append(c); i += 1; continue
                i += 1; state = "line"; removed += 1; continue
            if c == '"':
                out.append(c); i += 1; state = "string_d"; continue
            if c == "'":
                out.append(c); i += 1; state = "string_s"; continue
            out.append(c); i += 1
        elif state == "line":
            if c == "\n":
                out.append("\n"); i += 1; state = "normal"
            else:
                i += 1
        elif state == "rb_block":
            # =end debe estar al inicio de línea
            if c == "\n":
                out.append("\n"); i += 1
            elif c == "=" and src[i:i+4] == "=end":
                # Verificar que es inicio de línea
                j = len(out) - 1
                at_line_start = True
                while j >= 0:
                    if out[j] == "\n":
                        break
                    if out[j] not in " \t":
                        at_line_start = False
                        break
                    j -= 1
                if at_line_start:
                    i += 4
                    state = "normal"
                else:
                    i += 1
            else:
                i += 1
        elif state == "string_d":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == '"':
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "string_s":
            if c == "\\":
                if i+1 < n and src[i+1] == "\\":
                    out.append("\\\\"); i += 2
                elif i+1 < n and src[i+1] == "'":
                    out.append("\\'"); i += 2
                else:
                    out.append(c); i += 1
            elif c == "'":
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
    return "".join(out), removed


def strip_css(src: str) -> tuple[str, int]:
    """CSS: /* */ bloque, "..."/'...' strings.

    Preserva comentarios con prefijo !: /* !.
    """
    out: list[str] = []
    i, n = 0, len(src)
    state = "normal"
    block_depth = 0
    removed = 0
    while i < n:
        c = src[i]
        c2 = src[i+1] if i+1 < n else ""
        if state == "normal":
            if c == "/" and c2 == "*":
                if _peek_preserve_prefix(src, i + 2, n):
                    out.append(c); out.append(c2); i += 2; continue
                i += 2; state = "block"; block_depth = 1; removed += 1; continue
            if c == '"':
                out.append(c); i += 1; state = "string_d"; continue
            if c == "'":
                out.append(c); i += 1; state = "string_s"; continue
            out.append(c); i += 1
        elif state == "block":
            if c == "/" and c2 == "*":
                block_depth += 1; i += 2
            elif c == "*" and c2 == "/":
                block_depth -= 1; i += 2
                if block_depth == 0:
                    state = "normal"
            elif c == "\n":
                out.append("\n"); i += 1
            else:
                i += 1
        elif state == "string_d":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == '"':
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
        elif state == "string_s":
            if c == "\\":
                out.append(c)
                if i+1 < n:
                    out.append(src[i+1]); i += 2
                else:
                    i += 1
            elif c == "'":
                out.append(c); i += 1; state = "normal"
            else:
                out.append(c); i += 1
    return "".join(out), removed


# ════════════════════════════════════════════════════════════
#  Dispatcher
# ════════════════════════════════════════════════════════════

def strip_comments_for_lang(src: str, lang: str) -> tuple[str, int]:
    """Elimina comentarios del código según el lenguaje. Devuelve (texto, n_comentarios)."""
    if lang == "rust":
        return strip_rust(src)
    if lang in ("js", "ts", "java", "c", "cpp", "kotlin", "go", "swift", "php", "lua"):
        has_template = lang in ("js", "ts")
        has_regex = lang in ("js", "ts")
        return strip_c_like(src, has_template=has_template, has_regex=has_regex)
    if lang in ("toml", "yaml"):
        return strip_hash_line(src, supports_triple=(lang == "toml"))
    if lang in ("shell", "py"):
        return strip_hash_line(src, supports_triple=(lang == "py"))
    if lang == "ruby":
        return strip_ruby(src)
    if lang in ("html", "md", "xml"):
        return strip_html_xml(src)
    if lang in ("css", "scss", "sass", "less"):
        return strip_css(src)
    if lang == "json":
        return src, 0
    if lang == "sql":
        # SQL: -- línea, /* */ bloque
        return strip_c_like(src, has_template=False, has_regex=False)
    # Por defecto: no hacer nada
    return src, 0


# ════════════════════════════════════════════════════════════
#  Detección binaria
# ════════════════════════════════════════════════════════════

def is_binary(path: Path, sniff_bytes: int = 8192) -> bool:
    try:
        with open(path, "rb") as f:
            chunk = f.read(sniff_bytes)
        if b"\x00" in chunk:
            return True
        # Heurística: muchos bytes no-textuales
        text_chars = bytes(range(0x20, 0x7F)) + b"\n\r\t\f\b"
        non_text = sum(1 for b in chunk if b not in text_chars)
        return len(chunk) > 0 and non_text / len(chunk) > 0.30
    except OSError:
        return True


# ════════════════════════════════════════════════════════════
#  Detección de HTML con <script>/<style> conteniendo <!-- literal
# ════════════════════════════════════════════════════════════

# Encuentra el contenido de un bloque <script>...</script> o <style>...</style>
# y comprueba si contiene la secuencia "<!--" como literal (no como comentario).
_SCRIPT_STYLE_RE = re.compile(
    r"<\s*(script|style)\b[^>]*>(.*?)<\s*/\s*\1\s*>",
    re.IGNORECASE | re.DOTALL,
)
_LITERAL_COMMENT_RE = re.compile(r"<!--")


def is_html_with_script_comments(content: str) -> bool:
    """True si el HTML contiene <!-- dentro de <script> o <style> (peligroso de tocar)."""
    for m in _SCRIPT_STYLE_RE.finditer(content):
        body = m.group(2)
        if _LITERAL_COMMENT_RE.search(body):
            return True
    return False


# ════════════════════════════════════════════════════════════
#  Iteración
# ════════════════════════════════════════════════════════════

def iter_source_files(
    roots: list[Path],
    exts: set[str] | None,
    excluded_dirs: set[str],
    excluded_files: set[str],
    include_lock: bool,
    no_exclude: bool,
    self_path: Path,
) -> Iterable[Path]:
    if exts is not None:
        exts = {e if e.startswith(".") else f".{e}" for e in exts}
    eff_excluded_dirs = set() if no_exclude else excluded_dirs
    eff_excluded_files = set() if (no_exclude or include_lock) else excluded_files

    for root in roots:
        if not root.exists():
            print(f"[!] Ruta inexistente: {root}", file=sys.stderr)
            continue
        for dirpath, dirnames, filenames in os.walk(root):
            # Filtrar directorios in-place
            dirnames[:] = [d for d in dirnames if d not in eff_excluded_dirs]
            for fname in filenames:
                p = Path(dirpath) / fname
                if p.resolve() == self_path.resolve():
                    continue
                if not no_exclude and fname in eff_excluded_files:
                    continue
                # Saltar minificados
                if not no_exclude and any(suffix in fname for suffix in MINIFIED_PATTERNS):
                    continue
                ext = p.suffix.lower()
                if exts is not None and ext not in exts:
                    continue
                if ext in BINARY_EXTS:
                    continue
                if is_binary(p):
                    continue
                yield p


# ════════════════════════════════════════════════════════════
#  CLI
# ════════════════════════════════════════════════════════════

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Elimina comentarios de un proyecto completo.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Ejemplos:\n"
            "  %(prog)s                          # dry-run sobre el directorio actual\n"
            "  %(prog)s --write                  # aplica cambios\n"
            "  %(prog)s src/ src-tauri/          # solo esos subdirectorios\n"
            "  %(prog)s --ext rs,js,ts           # limita extensiones\n"
            "  %(prog)s --include-lock           # incluye lockfiles\n"
            "  %(prog)s --no-exclude             # desactiva exclusiones por defecto\n"
            "  %(prog)s --include-dir build      # excluye también 'build' (aditivo)\n"
        ),
    )
    parser.add_argument(
        "roots", nargs="*", type=Path, default=[Path(".")],
        help="Directorios raíz a procesar (por defecto: directorio actual).",
    )
    parser.add_argument(
        "--ext", type=lambda s: {e.strip().lower() for e in s.split(",") if e.strip()},
        default=None,
        help="Lista separada por comas de extensiones a procesar (ej: rs,js,ts).",
    )
    parser.add_argument(
        "--write", action="store_true",
        help="Sobrescribe los archivos. Sin esta opción, solo muestra un resumen (dry-run).",
    )
    parser.add_argument(
        "--include-lock", action="store_true",
        help="Incluye lockfiles (Cargo.lock, package-lock.json, ...).",
    )
    parser.add_argument(
        "--include-dir", action="append", default=[],
        help="Añade un directorio a la lista de exclusión (puede repetirse).",
    )
    parser.add_argument(
        "--no-exclude", action="store_true",
        help="Desactiva todas las exclusiones por defecto.",
    )
    parser.add_argument(
        "--self", action="store_true",
        help="Permite procesar el propio script (peligroso: se auto-elimina).",
    )
    parser.add_argument(
        "--verbose", "-v", action="store_true",
        help="Muestra cada archivo procesado.",
    )
    parser.add_argument(
        "--quiet", "-q", action="store_true",
        help="Suprime todo excepto el resumen final.",
    )
    parser.add_argument(
        "--stats", action="store_true",
        help="Imprime estadísticas detalladas (comentarios por tipo, bytes ahorrados).",
    )
    parser.add_argument(
        "--unsafe-html", action="store_true",
        help="Procesa HTML aunque contenga <!-- dentro de <script>/<style> "
             "(puede romper código embebido). Por defecto se omiten por seguridad.",
    )

    args = parser.parse_args(argv)

    excluded_dirs = set(EXCLUDED_DIRS) | set(args.include_dir)
    excluded_files = set(EXCLUDED_FILE_NAMES)
    self_path = Path(__file__).resolve()

    # Determinar self_path excluible
    if args.self:
        self_path = Path("/__never__")  # Nunca coincidirá

    files = list(iter_source_files(
        roots=args.roots,
        exts=args.ext,
        excluded_dirs=excluded_dirs,
        excluded_files=excluded_files,
        include_lock=args.include_lock,
        no_exclude=args.no_exclude,
        self_path=self_path,
    ))

    if not args.quiet:
        print(f"→ Archivos a procesar: {len(files)}", file=sys.stderr)
        if not args.write:
            print("→ Modo DRY-RUN (sin cambios). Usa --write para aplicar.", file=sys.stderr)

    total_files_changed = 0
    total_comments = 0
    total_bytes_before = 0
    total_bytes_after = 0
    by_ext: dict[str, tuple[int, int]] = {}  # ext → (files, comments)

    for path in files:
        try:
            original = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError) as e:
            if not args.quiet:
                print(f"[skip] {path}: {e}", file=sys.stderr)
            continue
        ext = path.suffix.lower()
        lang = EXT_LANG.get(ext, "")
        # HTML/MD con <!-- dentro de <script>/<style>: omitir por seguridad
        if lang in ("html", "md", "xml") and not args.unsafe_html:
            if is_html_with_script_comments(original):
                if not args.quiet:
                    rel = path
                    try:
                        rel = path.relative_to(Path.cwd())
                    except ValueError:
                        pass
                    print(f"  [skip-html] {rel}  (contiene <!-- dentro de <script>/<style>)", file=sys.stderr)
                continue
        cleaned, n = strip_comments_for_lang(original, lang)
        if cleaned != original:
            total_files_changed += 1
            total_comments += n
            total_bytes_before += len(original.encode("utf-8"))
            total_bytes_after += len(cleaned.encode("utf-8"))
            f, c = by_ext.get(ext, (0, 0))
            by_ext[ext] = (f + 1, c + n)
            if args.verbose:
                rel = path
                try:
                    rel = path.relative_to(Path.cwd())
                except ValueError:
                    pass
                print(f"  [mod] {rel}  ({n} comentarios, {len(original)} → {len(cleaned)} bytes)", file=sys.stderr)
            if args.write:
                try:
                    path.write_text(cleaned, encoding="utf-8")
                except OSError as e:
                    print(f"[!] Error escribiendo {path}: {e}", file=sys.stderr)
        elif args.verbose:
            rel = path
            try:
                rel = path.relative_to(Path.cwd())
            except ValueError:
                pass
            print(f"  [=]   {rel}  (sin cambios)", file=sys.stderr)

    # Resumen
    if args.quiet:
        return 0

    print("", file=sys.stderr)
    print("═" * 60, file=sys.stderr)
    if args.write:
        print(f"✓ Escritos: {total_files_changed} archivos modificados", file=sys.stderr)
    else:
        print(f"○ Dry-run: {total_files_changed} archivos se modificarían", file=sys.stderr)
    print(f"  Comentarios eliminados: {total_comments}", file=sys.stderr)
    print(f"  Bytes: {total_bytes_before} → {total_bytes_after} "
          f"(ahorrados {total_bytes_before - total_bytes_after})", file=sys.stderr)

    if args.stats and by_ext:
        print("", file=sys.stderr)
        print("Por extensión:", file=sys.stderr)
        for ext in sorted(by_ext):
            f, c = by_ext[ext]
            lang = EXT_LANG.get(ext, "?")
            print(f"  {ext:8} ({lang:6}) archivos={f:4} comentarios={c}", file=sys.stderr)

    return 0


if __name__ == "__main__":
    sys.exit(main())
