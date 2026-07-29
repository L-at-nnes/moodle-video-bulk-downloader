const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const el = (id) => document.getElementById(id);

const STATUS_LABELS = {
  Queued: "En attente",
  Extracting: "Extraction",
  Downloading: "Téléchargement",
  Muxing: "Muxage",
  Retrying: "Nouvelle tentative",
  Done: "Terminé",
  Error: "Erreur",
};

const STATUS_CLASS = {
  Queued: "status-queued",
  Extracting: "status-extracting",
  Downloading: "status-downloading",
  Muxing: "status-muxing",
  Retrying: "status-retrying",
  Done: "status-done",
  Error: "status-error",
};

/** @type {Map<number, {course:string, url:string, title:string|null, status:string, audio:number, video:number, retry:number}>} */
let rows = new Map();
let running = false;

function bar(pct, width = 6) {
  const filled = Math.max(0, Math.min(width, Math.round((pct / 100) * width)));
  return "█".repeat(filled) + "░".repeat(width - filled);
}

function renderTable() {
  const body = el("video-table-body");
  const empty = el("table-empty");
  const scroll = document.querySelector(".table-scroll");
  const ordered = [...rows.entries()].sort((a, b) => a[0] - b[0]);

  if (ordered.length === 0) {
    body.innerHTML = "";
    scroll.style.display = "none";
    empty.style.display = "block";
    return;
  }
  scroll.style.display = "";
  empty.style.display = "none";

  body.innerHTML = ordered
    .map(([idx, row]) => {
      const label = row.title || row.url;
      const statusClass = STATUS_CLASS[row.status] || "";
      const statusLabel =
        row.status === "Retrying"
          ? `${STATUS_LABELS.Retrying} ${row.retry}/3`
          : STATUS_LABELS[row.status] || row.status;
      const av =
        row.status === "Downloading" || row.status === "Muxing"
          ? `${bar(row.audio)} ${bar(row.video)}`
          : "";
      return `<tr>
        <td class="col-idx">${idx + 1}</td>
        <td class="col-course" title="${escapeHtml(row.course)}">${escapeHtml(row.course)}</td>
        <td class="col-title" title="${escapeHtml(label)}">${escapeHtml(label)}</td>
        <td class="col-status"><span class="status ${statusClass}">${statusLabel}</span></td>
        <td class="col-av">${av}</td>
      </tr>`;
    })
    .join("");
}

function renderProgress() {
  const total = rows.size;
  const done = [...rows.values()].filter((r) => r.status === "Done" || r.status === "Error").length;
  const pct = total > 0 ? (done / total) * 100 : 0;
  el("global-progress-fill").style.width = `${pct}%`;
  el("global-progress-label").textContent = `${done} / ${total}`;
}

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function log(message, cls) {
  const output = el("log-output");
  const line = document.createElement("div");
  line.className = "log-line" + (cls ? ` ${cls}` : "");
  const ts = new Date().toLocaleTimeString("fr-CH");
  line.textContent = `${ts}  ${message}`;
  output.appendChild(line);
  output.scrollTop = output.scrollHeight;
}

function setRunning(value) {
  running = value;
  el("btn-start").disabled = value;
  el("btn-cancel").disabled = !value;
}

function collectOptions() {
  return {
    output_dir: el("output-dir").value.trim() || "dl",
    concurrency: Math.max(1, parseInt(el("concurrency").value, 10) || 1),
    download_threads: Math.max(1, parseInt(el("download-threads").value, 10) || 4),
    capture_wait_ms: Math.max(500, parseInt(el("capture-wait").value, 10) || 8000),
    ffmpeg_timeout_s: Math.max(60, parseInt(el("ffmpeg-timeout").value, 10) || 1800),
    headless: !el("opt-show-browser").checked,
    keep_temp: el("opt-keep-temp").checked,
    dry_run: el("opt-dry-run").checked,
    set_file_date: el("opt-set-file-date").checked,
  };
}

async function loadSettings() {
  try {
    const s = await invoke("load_settings");
    if (s.output_dir) el("output-dir").value = s.output_dir;
    if (s.concurrency) el("concurrency").value = s.concurrency;
    if (s.download_threads) el("download-threads").value = s.download_threads;
    if (s.capture_wait_ms) el("capture-wait").value = s.capture_wait_ms;
    if (s.ffmpeg_timeout_s) el("ffmpeg-timeout").value = s.ffmpeg_timeout_s;
    if (s.headless === false) el("opt-show-browser").checked = true;
    if (s.keep_temp) el("opt-keep-temp").checked = true;
    if (s.dry_run) el("opt-dry-run").checked = true;
    if (s.set_file_date === false) el("opt-set-file-date").checked = false;
  } catch (e) {
    console.error("load_settings failed", e);
  }
}

async function saveSettings() {
  const o = collectOptions();
  await invoke("save_settings", {
    settings: {
      output_dir: o.output_dir,
      concurrency: o.concurrency,
      download_threads: o.download_threads,
      capture_wait_ms: o.capture_wait_ms,
      ffmpeg_timeout_s: o.ffmpeg_timeout_s,
      headless: o.headless,
      keep_temp: o.keep_temp,
      dry_run: o.dry_run,
      set_file_date: o.set_file_date,
    },
  }).catch((e) => console.error("save_settings failed", e));
}

el("btn-pick-dir").addEventListener("click", async () => {
  const dir = await invoke("pick_output_dir");
  if (dir) el("output-dir").value = dir;
});

el("btn-load-cookies").addEventListener("click", async () => {
  const content = await invoke("pick_cookie_file").catch((e) => {
    log(`Impossible de charger le fichier de cookies : ${e}`, "err");
    return null;
  });
  if (content) el("cookies").value = content;
});

el("btn-start").addEventListener("click", async () => {
  const cookiesText = el("cookies").value.trim();
  const urlsText = el("urls").value.trim();
  if (!cookiesText) {
    log("Collez vos cookies avant de démarrer.", "err");
    return;
  }
  if (!urlsText) {
    log("Ajoutez au moins un lien avant de démarrer.", "err");
    return;
  }

  rows.clear();
  renderTable();
  renderProgress();
  setRunning(true);
  await saveSettings();

  try {
    await invoke("start_download", {
      payload: {
        urls_text: urlsText,
        cookies_text: cookiesText,
        options: collectOptions(),
      },
    });
  } catch (e) {
    setRunning(false);
    log(`Erreur au démarrage : ${e}`, "err");
  }
});

el("btn-cancel").addEventListener("click", async () => {
  await invoke("cancel_download");
  log("Annulation demandée…");
});

listen("mvbd://entries", (event) => {
  rows.clear();
  event.payload.forEach((entry, idx) => {
    rows.set(idx, { course: entry.course, url: entry.url, title: null, status: "Queued", audio: 0, video: 0, retry: 0 });
  });
  renderTable();
  renderProgress();
});

listen("mvbd://log", (event) => {
  log(event.payload);
});

listen("mvbd://progress", (event) => {
  const ev = event.payload;
  const row = rows.get(ev.idx);
  if (!row) return;

  switch (ev.type) {
    case "Status":
      row.status = ev.status;
      break;
    case "Title":
      row.title = ev.title;
      break;
    case "Progress":
      if (ev.audio !== null && ev.audio !== undefined) row.audio = ev.audio;
      if (ev.video !== null && ev.video !== undefined) row.video = ev.video;
      break;
    case "Retry":
      row.status = "Retrying";
      row.retry = ev.attempt;
      break;
    case "Done":
      row.status = "Done";
      row.audio = 100;
      row.video = 100;
      log(`✓ ${row.title || row.url}`, "ok");
      break;
    case "Error":
      row.status = "Error";
      log(`✗ ${row.title || row.url} : ${ev.message}`, "err");
      break;
  }

  renderTable();
  renderProgress();
});

listen("mvbd://finished", (event) => {
  setRunning(false);
  const payload = event.payload;
  if (payload.type === "Ok") {
    log(`Terminé : ${payload.success_count}/${rows.size} vidéo(s) téléchargée(s).`);
  } else if (payload.type === "Cancelled") {
    log("Téléchargement annulé.");
  } else {
    log(`Échec : ${payload.message}`, "err");
  }
});

renderTable();
renderProgress();
loadSettings();
