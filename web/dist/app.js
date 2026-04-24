// PNGres frontend. No external dependencies. ~200 lines of monke administration.

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

const fmtBytes = (n) => {
  if (!Number.isFinite(n)) return '—';
  if (n < 1024) return `${n} B`;
  const units = ['KiB', 'MiB', 'GiB', 'TiB'];
  let v = n / 1024;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) { v /= 1024; u += 1; }
  return `${v < 10 ? v.toFixed(2) : v.toFixed(1)} ${units[u]}`;
};

const fmtRelative = (unixSeconds) => {
  const delta = Date.now() / 1000 - unixSeconds;
  if (delta < 60) return 'just now';
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
};

const monkeWord = (n) => (n === 1 ? 'monke' : 'monkes');

// -----------------------------------------------------------------------------
// stats

async function refreshStats() {
  try {
    const r = await fetch('/api/stats');
    if (!r.ok) throw new Error(`stats ${r.status}`);
    const s = await r.json();
    $('#stat-monkes').textContent = s.total_monkes.toLocaleString();
    $('#stat-bytes').textContent = fmtBytes(Number(s.total_bytes));
    $('#stat-files').textContent = s.total_files.toLocaleString();
  } catch (e) {
    console.error('stats failed', e);
  }
}

// -----------------------------------------------------------------------------
// file list

function renderFiles(items) {
  const list = $('#file-list');
  const empty = $('#empty-state');
  list.innerHTML = '';
  if (!items.length) {
    empty.classList.remove('hidden');
    return;
  }
  empty.classList.add('hidden');
  for (const f of items) {
    const li = document.createElement('li');
    li.className = 'file';
    const isVideo = (f.mime || '').startsWith('video/');
    const isAudio = (f.mime || '').startsWith('audio/');

    const main = document.createElement('div');
    main.className = 'file-main';
    const name = document.createElement('div');
    name.className = 'name';
    name.textContent = f.name;
    const meta = document.createElement('div');
    meta.className = 'meta';
    const mCount = `<span class="m-count">${f.num_monkes} ${monkeWord(f.num_monkes)}</span>`;
    meta.innerHTML = `${mCount} · ${fmtBytes(Number(f.size))} · ${f.mime} · ${f.lsb_bits}-bit LSB · ${fmtRelative(f.created_at)}`;
    main.appendChild(name);
    main.appendChild(meta);

    const actions = document.createElement('div');
    actions.className = 'file-actions';

    if (isVideo || isAudio) {
      const play = document.createElement('button');
      play.className = 'play';
      play.textContent = isVideo ? 'play' : 'listen';
      play.onclick = () => openPlayer(f);
      actions.appendChild(play);
    }

    const dl = document.createElement('a');
    dl.className = 'download';
    dl.href = `/api/files/${f.id}/download`;
    dl.textContent = 'download';
    dl.setAttribute('download', f.name);
    actions.appendChild(dl);

    const del = document.createElement('button');
    del.className = 'delete';
    del.textContent = 'delete';
    del.onclick = () => deleteFile(f);
    actions.appendChild(del);

    li.appendChild(main);
    li.appendChild(actions);
    list.appendChild(li);
  }
}

async function refreshList() {
  try {
    const r = await fetch('/api/files');
    if (!r.ok) throw new Error(`list ${r.status}`);
    const d = await r.json();
    renderFiles(d.items || []);
  } catch (e) {
    console.error('list failed', e);
    toast('could not fetch the herd', 'err');
  }
}

async function deleteFile(f) {
  if (!confirm(`Return ${f.num_monkes} ${monkeWord(f.num_monkes)} to the wild? They will not remember you.`)) return;
  try {
    const r = await fetch(`/api/files/${f.id}`, { method: 'DELETE' });
    if (!r.ok) throw new Error(`delete ${r.status}`);
    toast(`${f.name} freed. the monkes are vibing elsewhere now.`, 'ok');
    await Promise.all([refreshList(), refreshStats()]);
  } catch (e) {
    toast('could not release the monke', 'err');
  }
}

// -----------------------------------------------------------------------------
// upload

function bindUpload() {
  const form = $('#upload-form');
  const input = $('#file-input');
  const dropArea = $('#drop-area');
  const chosen = $('#chosen-name');
  const btn = $('#upload-btn');
  const bar = $('#progress-bar');
  const wrap = $('#progress-wrap');
  const note = $('#upload-note');

  const setChosen = (file) => {
    chosen.textContent = file ? `chosen: ${file.name} (${fmtBytes(file.size)})` : '';
    btn.disabled = !file;
  };

  input.addEventListener('change', () => setChosen(input.files[0]));

  dropArea.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropArea.classList.add('drag');
  });
  dropArea.addEventListener('dragleave', () => dropArea.classList.remove('drag'));
  dropArea.addEventListener('drop', (e) => {
    e.preventDefault();
    dropArea.classList.remove('drag');
    if (e.dataTransfer.files.length) {
      input.files = e.dataTransfer.files;
      setChosen(input.files[0]);
    }
  });

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const file = input.files[0];
    if (!file) return;
    btn.disabled = true;
    wrap.hidden = false;
    bar.style.width = '0%';
    note.textContent = `Summoning monkes for ${file.name}... one is already bored.`;

    try {
      await new Promise((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        xhr.open('POST', '/api/upload');
        xhr.upload.addEventListener('progress', (ev) => {
          if (ev.lengthComputable) {
            const pct = Math.round((ev.loaded / ev.total) * 100);
            bar.style.width = `${pct}%`;
          }
        });
        xhr.onload = () => {
          if (xhr.status >= 200 && xhr.status < 300) resolve(xhr.response);
          else reject(new Error(`upload ${xhr.status}: ${xhr.responseText}`));
        };
        xhr.onerror = () => reject(new Error('network'));
        const fd = new FormData();
        fd.append('file', file);
        xhr.send(fd);
      });
      const resp = await fetch('/api/files'); // no-op to get the latest list
      const d = await resp.json();
      const first = d.items[0];
      toast(`${first.name} is now ${first.num_monkes} ${monkeWord(first.num_monkes)}. they are vibing.`, 'ok');
      note.textContent = '';
      input.value = '';
      setChosen(null);
      await Promise.all([refreshList(), refreshStats()]);
    } catch (err) {
      toast('a monke has fainted. check the server logs.', 'err');
      note.textContent = err.message;
    } finally {
      setTimeout(() => { wrap.hidden = true; bar.style.width = '0%'; }, 600);
      btn.disabled = !input.files[0];
    }
  });
}

// -----------------------------------------------------------------------------
// video / audio player

function openPlayer(f) {
  const dlg = $('#player');
  const v = $('#player-video');
  $('#player-title').textContent = `The Monkes Are Projecting — ${f.name}`;
  v.src = `/api/files/${f.id}/stream`;
  v.load();
  dlg.showModal();
  // Pause and clear when dialog closes.
  dlg.addEventListener('close', () => {
    v.pause();
    v.removeAttribute('src');
    v.load();
  }, { once: true });
}

function bindPlayer() {
  $('#player-close').onclick = () => $('#player').close();
  $('#player').addEventListener('click', (e) => {
    if (e.target.tagName === 'DIALOG') $('#player').close();
  });
}

// -----------------------------------------------------------------------------
// toast

function toast(msg, kind = '') {
  const el = document.createElement('div');
  el.className = `toast ${kind}`.trim();
  el.textContent = msg;
  document.body.appendChild(el);
  setTimeout(() => el.remove(), 3500);
}

// -----------------------------------------------------------------------------

document.addEventListener('DOMContentLoaded', () => {
  bindUpload();
  bindPlayer();
  $('#refresh-btn').onclick = () => Promise.all([refreshList(), refreshStats()]);
  refreshStats();
  refreshList();
  setInterval(refreshStats, 15000); // passive refresh
});
