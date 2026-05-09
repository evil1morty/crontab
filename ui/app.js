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
  output: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="8" y1="13" x2="16" y2="13"/><line x1="8" y1="17" x2="14" y2="17"/></svg>',
  search: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>',
};

const state = {
  tab: 'jobs',
  jobs: [],
  runs: [],
  master: true,
  autostart: false,
  configPath: '',
  form: { open: false, index: null, name: '', schedule: '*/5 * * * *', command: '', maxRuns: '', timeout: '', allowConcurrent: false, runOnStartup: false, error: null, valid: true },
  presets: [],
  maxHistory: 200,
  filter: '',
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

/* === Modal confirmation ===
 * Returns a promise that resolves to true (confirmed) / false (cancelled).
 * Esc cancels, Enter confirms, click on backdrop cancels.
 */
function confirmDialog(message, opts = {}) {
  const okLabel = opts.okLabel || 'Delete';
  const danger = opts.danger !== false;
  const previouslyFocused = document.activeElement;
  return new Promise((resolve) => {
    const overlay = document.createElement('div');
    overlay.className = 'modal-overlay';
    overlay.innerHTML = `
      <div class="modal" role="dialog" aria-modal="true" aria-labelledby="modal-msg">
        <div class="modal-body" id="modal-msg">${escHtml(message)}</div>
        <div class="modal-actions">
          <button class="btn btn-ghost" data-act="cancel" type="button">Cancel</button>
          <button class="btn ${danger ? 'btn-danger btn-confirm-danger' : 'btn-primary'}" data-act="ok" type="button">${escHtml(okLabel)}</button>
        </div>
      </div>`;
    const onKey = (e) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(false); }
      else if (e.key === 'Enter') { e.preventDefault(); close(true); }
      else if (e.key === 'Tab') {
        const btns = overlay.querySelectorAll('button');
        const first = btns[0], last = btns[btns.length - 1];
        if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    };
    function close(v) {
      document.removeEventListener('keydown', onKey, true);
      overlay.remove();
      if (previouslyFocused && previouslyFocused.focus) {
        try { previouslyFocused.focus(); } catch (_) {}
      }
      resolve(v);
    }
    overlay.addEventListener('click', (e) => {
      if (e.target === overlay) { close(false); return; }
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'ok') close(true);
      else if (act === 'cancel') close(false);
    });
    document.addEventListener('keydown', onKey, true);
    document.body.appendChild(overlay);
    overlay.querySelector('[data-act="ok"]').focus();
  });
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
async function loadMaxHistory() {
  state.maxHistory = await invoke('get_max_run_history');
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
    if (state.form.open) {
      $('#page-title').textContent = state.form.index === null ? 'New job' : 'Edit job';
      actions.innerHTML = '';
    } else {
      $('#page-title').textContent = 'Jobs';
      actions.innerHTML = `<button class="btn btn-primary" id="new-job">${SVG.plus}<span>New job</span></button>`;
      $('#new-job').addEventListener('click', openFormNew);
    }
    page.innerHTML = renderJobs();
    bindJobs();
  } else if (state.tab === 'logs') {
    await loadRuns();
    actions.innerHTML = `<button class="btn btn-secondary" id="open-logs">${SVG.folder}<span>Logs folder</span></button>`;
    $('#open-logs').addEventListener('click', () => invoke('open_logs_folder'));
    page.innerHTML = renderLogs();
  } else if (state.tab === 'settings') {
    await Promise.all([loadAutostart(), loadConfigPath(), loadMaxHistory()]);
    page.innerHTML = renderSettings();
    bindSettings();
  }
}

/* === Jobs === */
function renderJobs() {
  if (state.form.open) return renderForm();
  if (state.jobs.length === 0) {
    return `
      <div class="empty">
        <div class="empty-icon">${SVG.inbox}</div>
        <div class="empty-title">No jobs yet</div>
        <div class="empty-sub">Click <span class="kbd">New job</span> or press <span class="kbd">Ctrl+N</span> to add one.</div>
      </div>`;
  }

  const q = state.filter.trim().toLowerCase();
  const filtered = q
    ? state.jobs.filter(j => j.name.toLowerCase().includes(q) || j.command.toLowerCase().includes(q))
    : state.jobs;

  let html = `
    <div class="filter-bar">
      <div class="filter-input">
        ${SVG.search}
        <input type="text" id="jobs-filter" placeholder="Filter by name or command…" value="${escHtml(state.filter)}"/>
      </div>
      <span class="muted" style="font-size:11.5px;">${filtered.length} of ${state.jobs.length}</span>
    </div>`;

  if (filtered.length === 0) {
    html += `<div class="empty"><div class="empty-title">No matches</div><div class="empty-sub">No job name or command contains "${escHtml(state.filter)}".</div></div>`;
  } else {
    html += '<div class="job-list">';
    for (const j of filtered) html += jobCard(j);
    html += '</div>';
  }
  return html;
}

function jobCard(j) {
  const cronClass = j.schedule_valid ? 'cron-pill' : 'cron-pill is-invalid';
  const next = j.schedule_valid && j.next_human && !j.exhausted
    ? `next: ${escHtml(j.next_display)} · ${escHtml(j.next_human)}`
    : (j.exhausted ? 'auto-disabled, run limit reached' : 'invalid schedule');

  let pills = '';
  if (j.max_runs !== null && j.max_runs !== undefined) {
    const cls = j.exhausted ? 'count-pill is-exhausted' : 'count-pill';
    pills += `<span class="${cls}" title="Auto-disable after ${j.max_runs} runs">${j.runs_count} / ${j.max_runs}</span>`;
  } else if (j.runs_count > 0) {
    pills += `<span class="count-pill is-muted" title="Total runs">${j.runs_count}×</span>`;
  }
  if (j.timeout_secs) pills += `<span class="count-pill is-muted" title="Killed after ${j.timeout_secs}s">⏱ ${j.timeout_secs}s</span>`;
  if (j.run_on_startup) pills += `<span class="count-pill is-muted" title="Runs once when Window Crontab launches">↻ on launch</span>`;
  if (j.allow_concurrent) pills += `<span class="count-pill is-muted" title="Concurrent runs allowed">∥ concurrent</span>`;
  if (j.is_running) pills += `<span class="count-pill is-running" title="A run is in progress"><span class="dot-inline"></span> running</span>`;

  return `
    <div class="card ${j.enabled ? '' : 'is-disabled'}" data-index="${j.index}">
      <div class="job-row1">
        <label class="toggle"><input type="checkbox" ${j.enabled ? 'checked' : ''} data-action="toggle"/><span class="slider"></span></label>
        <span class="job-name">${escHtml(j.name)}</span>
        <span class="${cronClass}">${escHtml(j.schedule)}</span>
        ${pills}
        <div class="job-actions">
          <button class="btn btn-icon btn-ghost" data-action="run" title="Run now" aria-label="Run now">${SVG.play}</button>
          <button class="btn btn-icon btn-ghost" data-action="output" title="View output" aria-label="View output log">${SVG.output}</button>
          <button class="btn btn-icon btn-ghost" data-action="edit" title="Edit" aria-label="Edit job">${SVG.edit}</button>
          <button class="btn btn-icon btn-danger" data-action="delete" title="Delete" aria-label="Delete job">${SVG.trash}</button>
        </div>
      </div>
      <div class="job-cmd">${escHtml(j.command)}</div>
      <div class="job-meta">${SVG.clock}<span>${next}</span></div>
    </div>`;
}

function bindJobs() {
  const filterEl = $('#jobs-filter');
  if (filterEl) {
    filterEl.addEventListener('input', () => {
      state.filter = filterEl.value;
      // re-render the page only, keep focus in the input
      const page = $('#page');
      page.innerHTML = renderJobs();
      bindJobs();
      const f2 = $('#jobs-filter');
      if (f2) { f2.focus(); f2.setSelectionRange(state.filter.length, state.filter.length); }
    });
  }
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
        const job = state.jobs.find(x => x.index === idx);
        const ok = await confirmDialog(
          `Delete "${job?.name ?? 'this job'}"? This can't be undone.`,
          { okLabel: 'Delete' }
        );
        if (!ok) return;
        await invoke('delete_job', { index: idx });
        toast('job deleted');
        render();
      } else if (action === 'run') {
        const name = await invoke('run_job_now', { index: idx });
        toast(`ran '${name}' now`);
      } else if (action === 'output') {
        try { await invoke('view_job_log', { index: idx }); }
        catch (err) { toast(String(err), true); }
      }
    });
  });
  if (state.form.open) bindForm();
}

/* === Form === */
function openFormNew() {
  state.form = { open: true, index: null, name: '', schedule: '*/5 * * * *', command: '', maxRuns: '', timeout: '', allowConcurrent: false, runOnStartup: false, error: null, valid: true };
  render();
}
function openFormEdit(idx) {
  const j = state.jobs.find(x => x.index === idx);
  if (!j) return;
  state.form = {
    open: true,
    index: idx,
    name: j.name,
    schedule: j.schedule,
    command: j.command,
    maxRuns: j.max_runs == null ? '' : String(j.max_runs),
    timeout: j.timeout_secs == null ? '' : String(j.timeout_secs),
    allowConcurrent: !!j.allow_concurrent,
    runOnStartup: !!j.run_on_startup,
    error: null,
    valid: true,
  };
  render();
}
function closeForm() {
  state.form.open = false;
  render();
}

function renderForm() {
  const f = state.form;
  return `
    <div class="form-card" id="form">
      <h3 class="form-title">${f.index === null ? 'New job' : 'Edit job'}</h3>
      <div class="field">
        <label class="field-label">Name</label>
        <input class="input" id="f-name" type="text" placeholder="Daily backup" value="${escHtml(f.name)}" autofocus/>
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
        <textarea class="textarea is-mono" id="f-command" placeholder="cd C:\\path\\to\\project && npm run cron">${escHtml(f.command)}</textarea>
      </div>
      <div class="form-grid">
        <div class="field">
          <label class="field-label">Auto-disable after N runs <span class="muted">(optional)</span></label>
          <input class="input is-mono" id="f-maxruns" type="number" min="1" placeholder="unlimited" value="${escHtml(f.maxRuns)}"/>
          ${f.index !== null ? `<button type="button" class="btn btn-ghost" id="f-reset-count" style="align-self:flex-start;padding:4px 8px;font-size:11.5px;margin-top:4px;">Reset run counter</button>` : ''}
        </div>
        <div class="field">
          <label class="field-label">Timeout in seconds <span class="muted">(optional)</span></label>
          <input class="input is-mono" id="f-timeout" type="number" min="1" placeholder="no timeout" value="${escHtml(f.timeout)}"/>
        </div>
      </div>
      <div class="field toggles">
        <label class="toggle-row">
          <span class="toggle"><input type="checkbox" id="f-startup" ${f.runOnStartup ? 'checked' : ''}/><span class="slider"></span></span>
          <span>Run when Window Crontab launches</span>
        </label>
        <label class="toggle-row">
          <span class="toggle"><input type="checkbox" id="f-concurrent" ${f.allowConcurrent ? 'checked' : ''}/><span class="slider"></span></span>
          <span>Allow concurrent runs <span class="muted">(by default a tick is skipped if the previous run is still alive)</span></span>
        </label>
      </div>
      <div class="form-row form-actions">
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

  nameEl.addEventListener('input', () => { f.name = nameEl.value; nameEl.classList.remove('is-invalid'); clearFormError(); });
  cmdEl.addEventListener('input', () => { f.command = cmdEl.value; cmdEl.classList.remove('is-invalid'); clearFormError(); });
  schedEl.addEventListener('input', () => { f.schedule = schedEl.value; updateNextPreview(); clearFormError(); });
  updateNextPreview();

  $('#f-save').addEventListener('click', async () => {
    const maxEl = $('#f-maxruns');
    const timeoutEl = $('#f-timeout');
    const startupEl = $('#f-startup');
    const concurrentEl = $('#f-concurrent');

    f.name = nameEl.value;
    f.schedule = schedEl.value;
    f.command = cmdEl.value;
    f.maxRuns = maxEl.value;
    f.timeout = timeoutEl.value;
    f.runOnStartup = startupEl.checked;
    f.allowConcurrent = concurrentEl.checked;

    nameEl.classList.remove('is-invalid');
    cmdEl.classList.remove('is-invalid');
    maxEl.classList.remove('is-invalid');
    timeoutEl.classList.remove('is-invalid');

    const missing = [];
    if (!f.name.trim())    { nameEl.classList.add('is-invalid'); missing.push('name'); }
    if (!f.command.trim()) { cmdEl.classList.add('is-invalid');  missing.push('command'); }
    if (missing.length) {
      const msg = `${missing.join(' and ')} ${missing.length === 1 ? 'is' : 'are'} required`;
      f.error = msg;
      $('#f-error').textContent = msg;
      (missing[0] === 'name' ? nameEl : cmdEl).focus();
      return;
    }
    const parsePos = (raw, el, label) => {
      if (raw.trim() === '') return null;
      const n = Number(raw);
      if (!Number.isFinite(n) || n < 1 || !Number.isInteger(n)) {
        el.classList.add('is-invalid');
        throw new Error(`${label} must be a positive integer`);
      }
      return n;
    };
    let maxRuns, timeoutSecs;
    try {
      maxRuns = parsePos(f.maxRuns, maxEl, 'max runs');
      timeoutSecs = parsePos(f.timeout, timeoutEl, 'timeout');
    } catch (err) {
      f.error = String(err.message || err);
      $('#f-error').textContent = f.error;
      return;
    }

    try {
      await invoke('save_job', { index: f.index, job: {
        name: f.name, schedule: f.schedule, command: f.command,
        enabled: true,
        max_runs: maxRuns,
        timeout_secs: timeoutSecs,
        allow_concurrent: f.allowConcurrent,
        run_on_startup: f.runOnStartup,
      } });
      toast(f.index === null ? 'job added' : 'job updated');
      closeForm();
    } catch (e) {
      f.error = String(e);
      $('#f-error').textContent = f.error;
      // schedule errors come from cron_validate, highlight the schedule field too
      schedEl.classList.add('is-invalid');
    }
  });
  $('#f-cancel').addEventListener('click', closeForm);
  const resetBtn = $('#f-reset-count');
  if (resetBtn) {
    resetBtn.addEventListener('click', async () => {
      try {
        await invoke('reset_runs_count', { index: f.index });
        toast('run counter reset');
      } catch (e) { toast(String(e), true); }
    });
  }

  setTimeout(() => nameEl.focus(), 30);
}

function clearFormError() {
  const el = document.getElementById('f-error');
  if (el) el.textContent = '';
  if (state.form) state.form.error = null;
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
  const STATUS = {
    ok:      { dot: '',          pill: 'ok',      label: 'ok' },
    fail:    { dot: 'is-fail',    pill: 'fail',    label: 'fail' },
    timeout: { dot: 'is-fail',    pill: 'fail',    label: 'timeout' },
    skipped: { dot: 'is-paused',  pill: 'muted',   label: 'skipped' },
  };
  let html = '<div class="log-list">';
  for (const r of state.runs) {
    const s = STATUS[r.status] || STATUS.fail;
    const codeText = r.status === 'skipped' ? 'skipped' : (r.status === 'timeout' ? 'timeout' : `exit ${r.exit_code}`);
    html += `
      <div class="log-row">
        <span class="dot ${s.dot}"></span>
        <span class="name">${escHtml(r.name)}</span>
        <span class="when">${escHtml(r.when)}</span>
        <span class="exit-pill ${s.pill}">${codeText}</span>
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
        <h3 class="card-title">History</h3>
        <p class="card-help">How many recent runs to keep on the Logs tab. Older runs are dropped. Per-job log files on disk are kept separately.</p>
        <div class="row" style="margin-top:10px;align-items:center;">
          <input class="input is-mono" id="s-max-history" type="number" min="1" style="max-width:120px;" value="${state.maxHistory}"/>
          <button class="btn btn-secondary" id="s-max-history-save">Save</button>
        </div>
      </div>

      <div class="card">
        <h3 class="card-title">Files</h3>
        <p class="card-help">Config is hot-reloaded. Changes on disk are picked up at the next minute boundary.</p>
        <div class="row" style="margin-top:10px;">
          <button class="btn btn-secondary" id="s-open-config">Open config file</button>
          <button class="btn btn-secondary" id="s-open-logs">Open logs folder</button>
        </div>
        <div class="path-mono">${escHtml(state.configPath)}</div>
      </div>

      <div class="card">
        <h3 class="card-title">App</h3>
        <p class="card-help">Quit Window Crontab and stop scheduling. Use <span class="kbd">Ctrl+W</span> to hide instead.</p>
        <div class="row" style="margin-top:10px;">
          <button class="btn btn-danger btn-secondary" id="s-quit" style="border-color:var(--danger);color:var(--danger);">Quit Window Crontab</button>
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
      toast(`autostart error: ${err}`, true);
      e.target.checked = !e.target.checked;
    }
  });
  $('#s-open-config').addEventListener('click', () => invoke('open_config_file'));
  $('#s-open-logs').addEventListener('click', () => invoke('open_logs_folder'));
  $('#s-quit').addEventListener('click', () => invoke('quit_app'));
  $('#s-max-history-save').addEventListener('click', async () => {
    const v = Number($('#s-max-history').value);
    if (!Number.isInteger(v) || v < 1) { toast('must be a positive integer', true); return; }
    try {
      await invoke('set_max_run_history', { value: v });
      state.maxHistory = v;
      toast(`history capped at ${v}`);
    } catch (e) { toast(String(e), true); }
  });
}

/* === Keyboard === */
document.addEventListener('keydown', (e) => {
  // Esc closes the form first, then hides the window
  if (e.key === 'Escape') {
    if (state.form.open) { closeForm(); return; }
    invoke('hide_window');
    return;
  }
  // Ctrl+N opens a new job form
  if (e.ctrlKey && (e.key === 'n' || e.key === 'N')) {
    e.preventDefault();
    if (state.tab !== 'jobs') setTab('jobs');
    openFormNew();
    return;
  }
  // Ctrl+W hides the window
  if (e.ctrlKey && (e.key === 'w' || e.key === 'W')) {
    e.preventDefault();
    invoke('hide_window');
    return;
  }
  // 1/2/3 switch tabs when not typing
  const isTyping = ['INPUT','TEXTAREA'].includes(document.activeElement?.tagName);
  if (!isTyping && (e.key === '1' || e.key === '2' || e.key === '3')) {
    setTab(['jobs','logs','settings'][Number(e.key) - 1]);
  }
});

/* === Live updates === */
listen('jobs-changed', () => { if (state.tab === 'jobs') render(); });
listen('config-changed', () => { loadMaster(); });

// Re-render the visible tab every 5s so "next run in Xm" stays fresh and
// finished jobs disappear from the running set. Skipped when:
//  - window is hidden (browsers report this even when a Tauri webview is
//    minimized to tray on most platforms)
//  - the form is open (don't blow away in-flight typing)
//  - the user is typing in the search filter (full re-render would steal
//    focus and the caret position mid-keystroke)
setInterval(() => {
  if (document.hidden) return;
  if (state.form.open) return;
  if (document.activeElement && document.activeElement.id === 'jobs-filter') return;
  if (state.tab === 'jobs' || state.tab === 'logs') render();
}, 5000);

/* === Theme ===
 * Default = system (prefers-color-scheme). The user's explicit toggle is
 * persisted to localStorage; until they toggle, the OS preference wins
 * and live changes (Settings → Personalization on Windows) are followed.
 */
function applyTheme(theme, persist) {
  if (theme === 'light') document.documentElement.setAttribute('data-theme', 'light');
  else document.documentElement.removeAttribute('data-theme');
  if (persist) {
    try { localStorage.setItem('crontab-theme', theme); } catch (e) {}
  }
}
$('#theme-toggle').addEventListener('click', () => {
  const next = document.documentElement.getAttribute('data-theme') === 'light' ? 'dark' : 'light';
  applyTheme(next, true);
});

if (window.matchMedia) {
  const mq = window.matchMedia('(prefers-color-scheme: light)');
  const onSystemThemeChange = (e) => {
    let stored = null;
    try { stored = localStorage.getItem('crontab-theme'); } catch (_) {}
    if (stored !== 'light' && stored !== 'dark') {
      applyTheme(e.matches ? 'light' : 'dark', false);
    }
  };
  if (mq.addEventListener) mq.addEventListener('change', onSystemThemeChange);
  else if (mq.addListener) mq.addListener(onSystemThemeChange);
}

/* === Boot === */
(async function init() {
  await Promise.all([loadMaster(), loadJobs(), loadPresets()]);
  await render();
})();
