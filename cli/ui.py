import threading
from dataclasses import dataclass
from datetime import datetime
from typing import Optional

from rich.console import Console, Group
from rich.live import Live
from rich.panel import Panel
from rich.progress import BarColumn, MofNCompleteColumn, Progress, TextColumn, TimeElapsedColumn
from rich.table import Table
from rich.text import Text


@dataclass
class _State:
    idx: int
    url: str
    course: str
    title: Optional[str] = None
    status: str = "queued"
    audio_pct: float = 0.0
    video_pct: float = 0.0
    retry_count: int = 0


_STATUS = {
    "queued":      ("·", "dim"),
    "extracting":  ("⟳", "yellow"),
    "downloading": ("↓", "cyan"),
    "muxing":      ("⚙", "blue"),
    "retry":       ("↻", "yellow"),
    "done":        ("✓", "green bold"),
    "error":       ("✗", "red bold"),
}

_LOG_MAX = 20
_LOG_DISPLAY = 5

# Lines consumed by non-table panels:
# Header(3) + Progress(4) + Log(_LOG_DISPLAY+3) + Table borders/title/header(4) + safety(2)
_FIXED_LINES = 3 + 4 + (_LOG_DISPLAY + 3) + 4 + 2


class DownloadUI:
    def __init__(
        self,
        entries,
        concurrency: int,
        dl_threads: int,
        auth_mode: str,
        console: Optional[Console] = None,
    ) -> None:
        n = len(entries)
        self._lock = threading.Lock()
        self._states = [_State(i, e.url, e.course) for i, e in enumerate(entries)]
        self._logs: list[str] = []

        self._prog = Progress(
            TextColumn("[bold blue]{task.description}"),
            BarColumn(bar_width=None),
            MofNCompleteColumn(),
            TextColumn("{task.percentage:>3.0f}%"),
            TimeElapsedColumn(),
            expand=True,
        )
        self._tid = self._prog.add_task("Global", total=n)

        ns = f"{n} vidéo{'s' if n > 1 else ''}"
        ws = f"{concurrency} worker{'s' if concurrency > 1 else ''}"
        ts = f"{dl_threads} thread{'s' if dl_threads > 1 else ''} ffmpeg"
        self._hdr = (
            f"[bold cyan]Moodle Video Downloader[/bold cyan]  "
            f"[dim]|[/dim]  {ns}  [dim]|[/dim]  auth=[white]{auth_mode}[/white]  "
            f"[dim]|[/dim]  {ws}  [dim]|[/dim]  {ts}"
        )
        self._console = console or Console()
        self._live = Live(
            self._render(),
            console=self._console,
            refresh_per_second=10,
            transient=False,
        )

    def __enter__(self) -> "DownloadUI":
        self._live.start(refresh=True)
        return self

    def __exit__(self, *_) -> None:
        self._live.update(self._render())
        self._live.stop()

    # ── thread-safe mutations ─────────────────────────────────────────────────

    def set_status(self, idx: int, status: str) -> None:
        with self._lock:
            self._states[idx].status = status
        self._live.update(self._render())

    def set_title(self, idx: int, title: str) -> None:
        with self._lock:
            self._states[idx].title = title
        self._live.update(self._render())

    def set_progress(self, idx: int, *, audio: Optional[float] = None, video: Optional[float] = None) -> None:
        with self._lock:
            s = self._states[idx]
            if audio is not None:
                s.audio_pct = audio
            if video is not None:
                s.video_pct = video
        self._live.update(self._render())

    def mark_retry(self, idx: int, attempt: int) -> None:
        with self._lock:
            s = self._states[idx]
            s.status = "retry"
            s.retry_count = attempt
        self._live.update(self._render())

    def mark_done(self, idx: int, output_path: str) -> None:
        ts = datetime.now().strftime("%H:%M:%S")
        with self._lock:
            s = self._states[idx]
            s.status = "done"
            s.audio_pct = s.video_pct = 100.0
            self._prog.advance(self._tid)
            label = s.title or s.url
            self._logs.append(f"[dim]{ts}[/dim]  [green bold]✓[/green bold]  {label}")
            self._trim()
        self._live.update(self._render())

    def mark_error(self, idx: int, msg: str) -> None:
        ts = datetime.now().strftime("%H:%M:%S")
        with self._lock:
            s = self._states[idx]
            s.status = "error"
            self._prog.advance(self._tid)
            label = s.title or s.url
            self._logs.append(
                f"[dim]{ts}[/dim]  [red bold]✗[/red bold]  {label}: [dim red]{msg[:70]}[/dim red]"
            )
            self._trim()
        self._live.update(self._render())

    def _trim(self) -> None:
        if len(self._logs) > _LOG_MAX:
            del self._logs[:-_LOG_MAX]

    # ── rendering ─────────────────────────────────────────────────────────────

    def _render(self) -> Group:
        with self._lock:
            snap = [
                (s.idx, s.course, s.title, s.url, s.status, s.audio_pct, s.video_pct, s.retry_count)
                for s in self._states
            ]
            logs = list(self._logs)

        # Compute how many table rows fit on screen
        term_h = self._console.size.height or 40
        max_rows = max(3, term_h - _FIXED_LINES)
        total = len(snap)

        if total <= max_rows:
            visible = snap
            v_start, v_end = 0, total
        else:
            # Keep viewport on last active row, else scroll to bottom
            focus = total - 1
            for i, row in enumerate(snap):
                if row[4] in ("extracting", "downloading", "muxing", "retry"):
                    focus = i
            v_start = max(0, min(focus - max_rows // 3, total - max_rows))
            v_end = v_start + max_rows
            visible = snap[v_start:v_end]

        tbl = Table(
            show_header=True,
            header_style="bold dim",
            show_edge=True,
            padding=(0, 1),
            expand=True,
        )
        tbl.add_column("#", style="dim", width=4, justify="right")
        tbl.add_column("Cours", max_width=22, no_wrap=True)
        tbl.add_column("Titre", ratio=3, no_wrap=True)
        tbl.add_column("État", min_width=16, no_wrap=True)
        tbl.add_column("A / V", width=13, justify="center")

        for idx, course, title, url, status, apct, vpct, retry_n in visible:
            icon, sty = _STATUS.get(status, ("?", "white"))
            label = title if title else f"[dim]{url[:50]}[/dim]"

            if status == "downloading":
                avg = (apct + vpct) / 2
                state_cell = Text.from_markup(f"[{sty}]{icon}[/{sty}] [cyan]{avg:3.0f}%[/cyan]")
                av_cell = Text.from_markup(
                    f"[cyan]{_bar(apct, 5)}[/cyan][dim]·[/dim][cyan]{_bar(vpct, 5)}[/cyan]"
                )
            elif status in ("done", "error"):
                lbl = "Terminé" if status == "done" else "Erreur"
                state_cell = Text.from_markup(f"[{sty}]{icon} {lbl}[/{sty}]")
                av_cell = Text("")
            elif status == "queued":
                state_cell = Text.from_markup(f"[{sty}]En attente[/{sty}]")
                av_cell = Text("")
            elif status == "extracting":
                state_cell = Text.from_markup(f"[{sty}]{icon} Extraction[/{sty}]")
                av_cell = Text("")
            elif status == "muxing":
                state_cell = Text.from_markup(f"[{sty}]{icon} Muxage[/{sty}]")
                av_cell = Text("")
            elif status == "retry":
                state_cell = Text.from_markup(f"[{sty}]{icon} Retry {retry_n}/3[/{sty}]")
                av_cell = Text("")
            else:
                state_cell = Text(status)
                av_cell = Text("")

            tbl.add_row(str(idx + 1), course[:22], label, state_cell, av_cell)

        # Table panel title with scroll position if rows are hidden
        if total > max_rows:
            above, below = v_start, total - v_end
            nav = f"({v_start + 1}–{v_end} / {total}"
            if above:
                nav += f"  ↑{above}"
            if below:
                nav += f"  ↓{below}"
            nav += ")"
            table_title = f"[bold]Vidéos[/bold] [dim]{nav}[/dim]"
        else:
            table_title = "[bold]Vidéos[/bold]"

        display_logs = logs[-_LOG_DISPLAY:]
        log_text = "\n".join(display_logs) if display_logs else "[dim]En attente…[/dim]"

        return Group(
            Panel(Text.from_markup(self._hdr), expand=True),
            Panel(self._prog, title="[bold]Progression globale[/bold]", expand=True, padding=(0, 1)),
            Panel(tbl, title=table_title, expand=True),
            Panel(Text.from_markup(log_text), title="[bold]Journal[/bold]", expand=True),
        )


def _bar(pct: float, w: int = 6) -> str:
    n = max(0, min(w, int(pct / 100 * w)))
    return "█" * n + "░" * (w - n)
