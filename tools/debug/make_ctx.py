"""Copy whole src/ tree into one flat markdown for feeding to an AI chat.

Uso:
    python make_ctx.py              # vuelca todo src/ (157 KB)
    python make_ctx.py --tiny       # solo train.rs + lib.rs + console.rs (~30 KB)
    python make_ctx.py --bins       # solo src/bin/*.rs
    python make_ctx.py --files a.rs b.rs

La salida es `src_context.md` en la raíz del proyecto, con encabezados
`## src/main.rs` y bloques de código, lista para pegar en un chat.
"""
from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent  # smb-rs/
SRC = ROOT / "src"
OUT = ROOT / "src_context.md"

IGNORE = {"target", "__pycache__", ".git"}

# subsets pequeños pero útiles
TINY = [
    "src/bin/train.rs",
    "src/bin/watchall.rs",
    "src/lib.rs",
    "src/console.rs",
    "src/cpu.rs",
    "Cargo.toml",
]


def strip_comments_and_md(code: str) -> str:
    """Quita comentarios // lineales y /// doc de cabecera de un bloque .rs,
    para no gastar tokens en prosa que la IA no necesita."""
    lines = []
    for line in code.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("//!") or stripped.startswith("///") or stripped.startswith("// "):
            continue
        # quita comentarios al final de línea // (cuidado con URLs en strings; simple)
        if "// " in line and stripped and not stripped.startswith("//"):
            # no tocar líneas con 'http' o strings con //
            if "://" not in line:
                line = line.split("// ")[0].rstrip()
        lines.append(line)
    return "\n".join(lines)


def collect(args: list[str]) -> tuple[list[Path], str]:
    base_note = ""
    if "--tiny" in args:
        files = []
        for rel in TINY:
            p = ROOT / rel
            if p.exists():
                files.append(p)
        base_note = "subset: tiny (train.rs + watchall.rs + lib + console + cpu)"
        args = [a for a in args if a != "--tiny"]
    elif "--bins" in args:
        files = sorted((SRC / "bin").glob("*.rs"))
        base_note = "subset: only src/bin/*.rs"
        args = [a for a in args if a != "--bins"]
    else:
        files = sorted(iter_files(SRC), key=lambda p: str(p.relative_to(ROOT)))
        if "--files" in args:
            i = args.index("--files")
            rest = args[i + 1 :]
            sel = []
            for r in rest:
                p = (ROOT / r) if not Path(r).is_absolute() else Path(r)
                if p.exists():
                    sel.append(p)
            files = sel or files
            base_note = "subset: --files " + ", ".join(rest)
        else:
            base_note = f"todo src/ ({len(files)} archivos)"
    return files, base_note


def main() -> int:
    if not SRC.exists():
        print(f"no src/ dir at {SRC}", file=sys.stderr)
        return 1

    args = sys.argv[1:]
    files, note = collect(args)
    if not files:
        print("nada que volcar", file=sys.stderr)
        return 1

    parts: list[str] = [
        "# src/ — contexto para IA\n",
        "Mensaje para la IA: below is the full source tree. Analyze for bugs\n"
        "in emulation, physics and RL. Consider the fitness/objective first.\n",
        f"Contenido: {note} ({len(files)} archivos)\n",
        "---\n",
    ]

    total_chars = 0
    for p in files:
        rel = p.relative_to(ROOT).as_posix() if p.is_relative_to(ROOT) else str(p)
        try:
            raw = p.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        # sin la cabecera `## src/bin/train.rs` y sin comentarios sueltos
        if rel.startswith("src/bin/"):
            rel = rel.removeprefix("src/bin/")
        txt = strip_comments_and_md(raw)
        total_chars += len(txt)
        parts.append(f"## {rel}\n")
        lang = "rust"
        if p.suffix == ".py":
            lang = "python"
        if p.suffix == ".toml":
            lang = "toml"
        parts.append(f"```{lang}\n{txt}\n```\n")

    cargo = ROOT / "Cargo.toml"
    if cargo not in files and cargo.exists():
        parts.append("## Cargo.toml\n")
        parts.append("```toml\n" + cargo.read_text(encoding="utf-8") + "\n```\n")

    OUT.write_text("\n".join(parts), encoding="utf-8")
    kb = OUT.stat().st_size // 1024
    print(f"wrote {OUT}  ({len(files)} files, {total_chars:,} chars, {kb} KB)")
    print(f"nota: {note}")
    if total_chars > 120_000:
        print("aviso: >120 KB — el chat puede cortarlo; prueba con --tiny")
    elif total_chars > 60_000:
        print("aviso: 60-120 KB — según el límite, quizá funcione; si no, --tiny")
    else:
        print("ok: tamaño razonable para la mayoría de chats")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
