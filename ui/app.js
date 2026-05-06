"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const SVG = {
  play:   '<svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>',
  edit:   '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z"/></svg>',
  trash:  '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/></svg>',
  clock:  '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 15 14"/></svg>',
  folder: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>',
  plus:   '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>',
  inbox:  '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/><path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/></svg>',
  history:'<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><polyline points="3 3 3 8 8 8"/><polyline points="12 7 12 12 15 13.5"/></svg>',
};

const state = {
  tab: 'jobs',
  jobs: [],
  runs: [],
  master: true,
  autostart: false,
  configPath: '',
  form: { open: false, index: null, name: '', schedule: '*/5 * * * *', command: '', error: null, valid: true },
  presets: [],
};

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

function escHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;' }[c]));
}

let toastTimer = null;
function toast(msg, isError = false) {
  const t = $('#toast');
  t.textContent = msg;
  t.classList.toggle('is-error', isError);
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, 2200);
}

/* === Data loaders === */
async function loadJobs() {
  state.jobs = await invoke('list_jobs');
  $('#jobs-count').textContent = state.jobs.length;
}
async function loadRuns() {
  state.runs = await invoke('list_runs');
}
async function loadMaster() {
  state.master = await invoke('get_master_enabled');
  renderMaster();
}
async function loadAutostart() {
  state.autostart = await invoke('is_autostart');
}
async function loadConfigPath() {
  state.configPath = await invoke('config_path_str');
}
async function loadPresets() {
  if (state.presets.length === 0) state.presets = await invoke('cron_presets');
}

/* === Master pill === */
function renderMaster() {
  $('#master-label').textContent = state.master ? 'Running' : 'Paused';
  $('#master-dot').classList.toggle('is-paused', !state.master);
}
$('#master-pill').addEventListener('click', async () => {
  state.master = !state.master;
  await invoke('set_master_enabled', { enabled: state.master });
  renderMaster();
});

/* === Tabs === */
function setTab(tab) {
  state.tab = tab;
  $$('.nav-item').forEach(b => b.classList.toggle('is-active', b.dataset.tab === tab));
  $('#page-title').textContent = tab[0].toUpperCase() + tab.slice(1);
  render();
}
$$('.nav-item').forEach(b => b.addEventListener('click', () => setTab(b.dataset.tab)));

/* === Render router === */
async function render() {
  const page = $('#page');
  const actions = $('#topbar-actions');
  actions.innerHTML = '';
  if (state.tab === 'jobs') {
    await loadJobs();
    actions.innerHTML = `<button class="btn btn-primary" id="new-job">${SVG.plus}<span>New job</span></button>`;
    $('#new-job').addEventListener('click', openFormNew);
    page.innerHTML = renderJobs();
    bindJobs();
  } else if (state.tab === 'logs') {
    await loadRuns();
    actions.innerHTML = `<button class="btn btn-secondary" id="open-logs">${SVG.folder}<span>Logs folder</span></button>`;
    $('#open-logs').addEventListener('click', () => invoke('open_logs_folder'));
    page.innerHTML = renderLogs();
  } else if (state.tab === 'settings') {
    await Promise.all([loadAutostart(), loadConfigPath()]);
    page.innerHTML = renderSettings();
    bindSettings();
  }
}

/* === Jobs === */
function renderJobs() {
  let html = '';
  if (state.form.open) html += renderForm();
  if (state.jobs.length === 0 && !state.form.open) {
    return `
      <div class="empty">
        <div class="empty-icon">${SVG.inbox}</div>
        <div class="empty-title">No jobs yet</div>
        <div class="empty-sub">Click <span class="kbd">New job</span> or press <span class="kbd">Ctrl+N</span> to add one.</div>
      </div>`;
  }
  html += '<div class="job-list">';
  for (const j of state.jobs) html += jobCard(j);
  html += '</div>';
  return html;
}

function jobCard(j) {
  const cronClass = j.schedule_valid ? 'cron-pill' : 'cron-pill is-invalid';
  const next = j.schedule_valid && j.next_human
    ? `next: ${escHtml(j.next_display)} · ${escHtml(j.next_human)}`
    : 'invalid schedule';
  return `
    <div class="card ${j.enabled ? '' : 'is-disabled'}" data-index="${j.index}">
      <div class="job-row1">
        <label class="toggle"><input type="checkbox" ${j.enabled ? 'checked' : ''} data-action="toggle"/><span class="slider"></span></label>
        <span class="job-name">${escHtml(j.name)}</span>
        <span class="${cronClass}">${escHtml(j.schedule)}</span>
        <div class="job-actions">
          <button class="btn btn-icon btn-ghost" data-action="run" title="Run now">${SVG.play}</button>
          <button class="btn btn-icon btn-ghost" data-action="edit" title="Edit">${SVG.edit}</button>
          <button class="btn btn-icon btn-danger" data-action="delete" title="Delete">${SVG.trash}</button>
        </div>
      </div>
      <div class="job-cmd">${escHtml(j.command)}</div>
      <div class="job-meta">${SVG.clock}<span>${next}</span></div>
    </div>`;
}

function bindJobs() {
  $$('.card[data-index]').forEach(card => {
    const idx = Number(card.dataset.index);
    card.addEventListener('click', async (e) => {
      const t = e.target.closest('[data-action]');
      if (!t) return;
      const action = t.dataset.action;
      if (action === 'toggle') {
        await invoke('toggle_job', { index: idx, enabled: t.checked });
      } else if (action === 'edit') {
        openFormEdit(idx);
      } else if (action === 'delete') {
        await invoke('delete_job', { index: idx });
        toast('job deleted');
        render();
      } else if (action === 'run') {
        const name = await invoke('run_job_now', { index: idx });
        toast(`ran '${name}' now`);
      }
    });
  });
  if (state.form.open) bindForm();
}

/* === Form === */
function openFormNew() {
  state.form = { open: true, index: null, name: '', schedule: '*/5 * * * *', command: '', error: null, valid: true };
  render();
}
function openFormEdit(idx) {
  const j = state.jobs.find(x => x.index === idx);
  if (!j) return;
  state.form = { open: true, index: idx, name: j.name, schedule: j.schedule, command: j.command, error: null, valid: true };
  render();
}
function closeForm() {
  state.form.open = false;
  render();
}

function renderForm() {
  loadPresets();
  const f = state.form;
  return `
    <div class="form-card" id="form">
      <h3 class="form-title">${f.index === null ? 'New job' : 'Edit job'}</h3>
      <div class="field">
        <label class="field-label">Name</label>
        <input class="input" id="f-name" type="text" placeholder="Backup database" value="${escHtml(f.name)}" autofocus/>
      </div>
      <div class="field">
        <label class="field-label">Schedule</label>
        <input class="input is-mono" id="f-schedule" type="text" value="${escHtml(f.schedule)}"/>
        <div class="next-preview" id="f-next"></div>
        <div class="preset-row" id="f-presets">
          <span class="preset-label">Presets:</span>
        </div>
      </div>
      <div class="field">
        <label class="field-label">Command</label>
        <textarea class="textarea is-mono" id="f-command" placeholder="echo hello">${escHtml(f.command)}</textarea>
      </div>
      <div class="form-row">
        <button class="btn btn-primary" id="f-save">${f.index === null ? 'Add' : 'Update'}</button>
        <button class="btn btn-ghost" id="f-cancel">Cancel <span class="kbd">Esc</span></button>
        <span class="spacer"></span>
        <span class="error-text" id="f-error">${f.error ? escHtml(f.error) : ''}</span>
      </div>
    </div>`;
}

function bindForm() {
  const f = state.form;
  const nameEl = $('#f-name');
  const schedEl = $('#f-schedule');
  const cmdEl = $('#f-command');

  // populate presets row
  const presetRow = $('#f-presets');
  for (const p of state.presets) {
    const chip = document.createElement('button');
    chip.className = 'chip';
    chip.type = 'button';
    chip.textContent = p.label;
    chip.addEventListener('click', () => { schedEl.value = p.expr; updateNextPreview(); });
    presetRow.appendChild(chip);
  }

  nameEl.addEventListener('input', () => { f.name = nameEl.value; });
  cmdEl.addEventListener('input', () => { f.command = cmdEl.value; });
  schedEl.addEventListener('input', () => { f.schedule = schedEl.value; updateNextPreview(); });
  updateNextPreview();

  $('#f-save').addEventListener('click', async () => {
    f.name = nameEl.value;
    f.schedule = schedEl.value;
    f.command = cmdEl.value;
    try {
      await invoke('save_job', { index: f.index, job: { name: f.name, schedule: f.schedule, command: f.command, enabled: true } });
      toast(f.index === null ? 'job added' : 'job updated');
      closeForm();
    } catch (e) {
      f.error = String(e);
      $('#f-error').textContent = f.error;
    }
  });
  $('#f-cancel').addEventListener('click', closeForm);

  setTimeout(() => nameEl.focus(), 30);
}

async function updateNextPreview() {
  const schedEl = $('#f-schedule');
  const nextEl = $('#f-next');
  if (!schedEl || !nextEl) return;
  try {
    const r = await invoke('cron_next', { expr: schedEl.value });
    nextEl.classList.remove('is-error');
    nextEl.textContent = `next: ${r.display} (${r.human})`;
    schedEl.classList.remove('is-invalid');
  } catch (e) {
    nextEl.classList.add('is-error');
    nextEl.textContent = String(e);
    schedEl.classList.add('is-invalid');
  }
}

/* === Logs === */
function renderLogs() {
  if (state.runs.length === 0) {
    return `
      <div class="empty">
        <div class="empty-icon">${SVG.history}</div>
        <div class="empty-title">No runs yet</div>
        <div class="empty-sub">Jobs will appear here after they fire.</div>
      </div>`;
  }
  let html = '<div class="log-list">';
  for (const r of state.runs) {
    html += `
      <div class="log-row">
        <span class="dot ${r.ok ? '' : 'is-fail'}"></span>
        <span class="name">${escHtml(r.name)}</span>
        <span class="when">${escHtml(r.when)}</span>
        <span class="exit-pill ${r.ok ? 'ok' : 'fail'}">exit ${r.exit_code}</span>
      </div>`;
  }
  html += '</div>';
  return html;
}

/* === Settings === */
function renderSettings() {
  return `
    <div class="settings">
      <div class="card">
        <h3 class="card-title">Startup</h3>
        <p class="card-help">Adds an entry to <span class="kbd">HKCU\\…\\Run</span> so the app launches with <span class="kbd">--hidden</span> on login.</p>
        <label class="checkbox-row" style="margin-top:10px;">
          <input type="checkbox" id="s-autostart" ${state.autostart ? 'checked' : ''}/>
          <span>Start with Windows (hidden in tray)</span>
        </label>
      </div>

      <div class="card">
        <h3 class="card-title">Files</h3>
        <p class="card-help">Config is hot-reloaded — changes on disk are picked up automatically.</p>
        <div class="row" style="margin-top:10px;">
          <button class="btn btn-secondary" id="s-open-config">Open config file</button>
          <button class="btn btn-secondary" id="s-open-logs">Open logs folder</button>
        </div>
        <div class="path-mono">${escHtml(state.configPath)}</div>
      </div>

      <div class="card">
        <h3 class="card-title">App</h3>
        <p class="card-help">Quit Crontab and stop scheduling. Use <span class="kbd">Ctrl+W</span> to hide instead.</p>
        <div class="row" style="margin-top:10px;">
          <button class="btn btn-danger btn-secondary" id="s-quit" style="border-color:var(--danger);color:var(--danger);">Quit Crontab</button>
        </div>
      </div>
    </div>`;
}

function bindSettings() {
  $('#s-autostart').addEventListener('change', async (e) => {
    try {
      await invoke('set_autostart', { enabled: e.target.checked });
      toast(e.target.checked ? 'autostart enabled' : 'autostart disabled');
    } catch (err) {
      toast(`autostart error — ${err}`, true);
      e.target.checked = !e.target.checked;
    }
  });
  $('#s-open-config').addEventListener('click', () => invoke('open_config_file'));
  $('#s-open-logs').addEventListener('click', () => invoke('open_logs_folder'));
  $('#s-quit').addEventListener('click', () => invoke('quit_app'));
}

/* === Keyboard === */
document.addEventListener('keydown', (e) => {
  // Esc — close form, then hide window
  if (e.key === 'Escape') {
    if (state.form.open) { closeForm(); return; }
    invoke('hide_window');
    return;
  }
  // Ctrl+N — new job
  if (e.ctrlKey && (e.key === 'n' || e.key === 'N')) {
    e.preventDefault();
    if (state.tab !== 'jobs') setTab('jobs');
    openFormNew();
    return;
  }
  // Ctrl+W — hide window
  if (e.ctrlKey && (e.key === 'w' || e.key === 'W')) {
    e.preventDefault();
    invoke('hide_window');
    return;
  }
  // 1/2/3 — switch tabs (when not typing)
  const isTyping = ['INPUT','TEXTAREA'].includes(document.activeElement?.tagName);
  if (!isTyping && (e.key === '1' || e.key === '2' || e.key === '3')) {
    setTab(['jobs','logs','settings'][Number(e.key) - 1]);
  }
});

/* === Live updates === */
listen('jobs-changed', () => { if (state.tab === 'jobs') render(); });
listen('config-changed', () => { loadMaster(); });

// Refresh "next run" labels every 30s and logs every 5s while visible.
setInterval(() => {
  if (document.hidden) return;
  if (state.tab === 'jobs' && !state.form.open) render();
  if (state.tab === 'logs') render();
}, 5000);

/* === Boot === */
(async function init() {
  await Promise.all([loadMaster(), loadJobs()]);
  await render();
})();
