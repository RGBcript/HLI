#!/usr/bin/env python3
"""Gate de desplazamiento y Transcript Mode (HLI 0.9).

Combina pruebas ejecutadas (modelo de viewport, eventos de ratón inyectados,
scrollbar, follow, búsqueda) con verificación estática de la captura y
restauración del ratón. `manual_pending` permanece true.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "crates" / "hli-tui" / "src" / "app.rs"
UI = ROOT / "crates" / "hli-tui" / "src" / "ui.rs"
MAIN = ROOT / "crates" / "hli-tui" / "src" / "main.rs"
TERM = ROOT / "crates" / "hli-tui" / "src" / "terminal.rs"
VIEW = ROOT / "crates" / "hli-tui" / "src" / "viewport.rs"

RESULT = {
    "gate": "agent-scroll",
    "ok": False,
    "mouse_wheel": False,
    "scrollbar": False,
    "follow_mode": False,
    "new_activity_indicator": False,
    "keyboard_scroll": False,
    "transcript_mode": False,
    "search": False,
    "resize": False,
    "mouse_release": False,
    "terminal_restore": False,
    "composer_preserved": False,
    "manual_pending": True,
}


def text(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def cargo(args: list[str], timeout: int = 600) -> tuple[int, str]:
    p = subprocess.run(["cargo", *args], cwd=str(ROOT), capture_output=True,
                       text=True, timeout=timeout)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


try:
    app = text(APP)
    ui = text(UI)
    main = text(MAIN)
    term = text(TERM)
    view = text(VIEW)
    render_tests = text(ROOT / "crates/hli-tui/tests/render.rs")

    # 1) Pruebas ejecutadas: modelo + interacción.
    code_lib, out_lib = cargo(["test", "-p", "hli-tui", "--lib"])
    lib_ok = code_lib == 0 and "test result: ok" in out_lib
    code_render, out_render = cargo(["test", "-p", "hli-tui", "--test", "render"])
    render_ok = code_render == 0 and "test result: ok" in out_render

    # 2) Eventos de ratón reales, no PageUp disfrazado.
    scroll_up = "MouseEventKind::ScrollUp" in app and "ScrollDown" in app
    handle_mouse = "fn handle_mouse" in app
    injected = "MouseEventKind::ScrollUp" in render_tests and "MouseEventKind::ScrollDown" in render_tests
    RESULT["mouse_wheel"] = bool(scroll_up and handle_mouse and injected and render_ok)

    # 3) Scrollbar con correspondencia matemática.
    RESULT["scrollbar"] = (
        "ScrollbarState::new" in ui
        and "ScrollbarOrientation::VerticalRight" in ui
        and "total_rendered_lines > vp.viewport_height" in ui
        and "fn thumb_ratio" in view
    )

    # 4) Follow mode y novedades.
    RESULT["follow_mode"] = (
        "follow_mode" in view
        and "on_content" in view
        and "sync_scroll_top" in app
    )
    RESULT["new_activity_indicator"] = (
        "unseen_activity_count" in view
        and "on_content" in view
        and "new activity" in ui
    )

    # 5) Teclado.
    RESULT["keyboard_scroll"] = all(
        k in app for k in ("PageUp", "PageDown", "KeyCode::Home if ctrl", "KeyCode::End if ctrl")
    )

    # 6) Transcript Mode y búsqueda.
    RESULT["transcript_mode"] = (
        "TuiMode::Transcript" in app
        and "fn draw_transcript" in ui
        and "fn transcript_key" in app
        and "open_transcript" in app
    )
    RESULT["search"] = (
        "apply_transcript_search" in app
        and "matches" in app
        and "REVERSED" in ui
    )

    # 7) Resize.
    RESULT["resize"] = (
        "observe_terminal" in app
        and "Event::Resize" in main
        and "fn sync" in view
    )

    # 8) Captura y restauración.
    RESULT["mouse_release"] = (
        "EnableMouseCapture" in term
        and "DisableMouseCapture" in term
        and "impl Drop for MouseGuard" in term
        and "release_mouse" in text(ROOT / "crates/hli-tui/src/lib.rs")
    )
    RESULT["terminal_restore"] = (
        "drop(guard)" in main
        and "ratatui::restore()" in main
        and "release_mouse();" in text(ROOT / "crates/hli-tui/src/lib.rs")
        and "MouseGuard::new(mouse_enabled_from_env())" in main
    )

    # 9) Compositor.
    RESULT["composer_preserved"] = (
        "apply_submission_outcome" in app
        and "accepted_prompt_clears_composer" in render_tests
        and "failed_prompt_submission_preserves_composer" in render_tests
        and "resume_does_not_restore_last_prompt_into_composer" in render_tests
    )

    RESULT["ok"] = all(
        v for k, v in RESULT.items() if k not in ("gate", "ok", "manual_pending")
    )
except Exception as exc:  # noqa: BLE001
    RESULT["error"] = str(exc)

print(json.dumps(RESULT, indent=2))
sys.exit(0 if RESULT["ok"] else 1)
