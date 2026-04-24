// PNGres frontend — activity stream, monke explorer, LSB visualizer.
// Zero external dependencies.

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));

// ------------------------------------------------------------------ utils

const fmtBytes = (n) => {
  if (!Number.isFinite(n)) return '—';
  if (n < 1024) return `${n} B`;
  const u = ['KiB', 'MiB', 'GiB', 'TiB'];
  let v = n / 1024, i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v < 10 ? v.toFixed(2) : v.toFixed(1)} ${u[i]}`;
};

const fmtRelative = (unixSeconds) => {
  const delta = Date.now() / 1000 - unixSeconds;
  if (delta < 10) return 'just now';
  if (delta < 60) return `${Math.floor(delta)}s ago`;
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
};

const fmtTime = (unixSeconds) => {
  const d = new Date(unixSeconds * 1000);
  return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
};

const monkeWord = (n) => (n === 1 ? 'monke' : 'monkes');
const escHtml = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

// ------------------------------------------------------------------ stats

async function refreshStats() {
  try {
    const r = await fetch('/api/stats');
    if (!r.ok) throw 0;
    const s = await r.json();
    $('#stat-monkes').textContent = s.total_monkes.toLocaleString();
    $('#stat-bytes').textContent = fmtBytes(Number(s.total_bytes));
    $('#stat-files').textContent = s.total_files.toLocaleString();
  } catch {}
}

// ------------------------------------------------------------------ files

const expanded = new Set(); // file_ids currently showing their herd grid

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
    li.dataset.id = f.id;
    const isVideo = (f.mime || '').startsWith('video/');
    const isAudio = (f.mime || '').startsWith('audio/');

    const peerLine = f.uploaded_from
      ? `<span class="peer">from ${escHtml(f.uploaded_from)}</span> · `
      : '';

    const rowMain = document.createElement('div');
    rowMain.className = 'row-main';
    rowMain.innerHTML = `
      <div class="file-main">
        <div class="name">${escHtml(f.name)}</div>
        <div class="meta">
          <span class="m-count">${f.num_monkes} ${monkeWord(f.num_monkes)}</span>
          · ${fmtBytes(Number(f.size))}
          · ${escHtml(f.mime)}
          · ${f.lsb_bits}-bit LSB
          · ${peerLine}${fmtRelative(f.created_at)}
        </div>
      </div>
      <div class="file-actions">
        ${(isVideo || isAudio) ? `<button class="play" data-action="play">${isVideo ? 'play' : 'listen'}</button>` : ''}
        <a class="download" href="/api/files/${f.id}/download" download="${escHtml(f.name)}">download</a>
        <button class="explore" data-action="explore">${expanded.has(f.id) ? 'hide herd' : 'explore herd'}</button>
        <button class="delete" data-action="delete">delete</button>
      </div>
    `;
    li.appendChild(rowMain);

    li.addEventListener('click', (e) => {
      const action = e.target.closest('[data-action]')?.dataset.action;
      if (!action) return;
      e.preventDefault();
      if (action === 'play') openPlayer(f);
      else if (action === 'download') { /* anchor default */ }
      else if (action === 'explore') toggleHerd(li, f);
      else if (action === 'delete') deleteFile(f);
    });

    if (expanded.has(f.id)) {
      mountHerd(li, f).catch(console.error);
    }

    list.appendChild(li);
  }
}

async function refreshList() {
  try {
    const r = await fetch('/api/files');
    if (!r.ok) throw 0;
    const d = await r.json();
    renderFiles(d.items || []);
  } catch {
    toast('could not fetch the herd', 'err');
  }
}

async function deleteFile(f) {
  if (!confirm(`Return ${f.num_monkes} ${monkeWord(f.num_monkes)} to the wild? They will not remember you.`)) return;
  try {
    const r = await fetch(`/api/files/${f.id}`, { method: 'DELETE' });
    if (!r.ok) throw 0;
    expanded.delete(f.id);
    toast(`${f.name} freed. the monkes are vibing elsewhere now.`, 'ok');
    await Promise.all([refreshList(), refreshStats()]);
  } catch {
    toast('could not release the monke', 'err');
  }
}

// ------------------------------------------------------------------ herd grid

async function toggleHerd(li, f) {
  if (expanded.has(f.id)) {
    expanded.delete(f.id);
    li.querySelector('.herd-grid')?.remove();
    li.querySelector('[data-action="explore"]').textContent = 'explore herd';
  } else {
    expanded.add(f.id);
    await mountHerd(li, f);
  }
}

async function mountHerd(li, f) {
  li.querySelector('.herd-grid')?.remove();
  const exploreBtn = li.querySelector('[data-action="explore"]');
  if (exploreBtn) exploreBtn.textContent = 'hide herd';
  const grid = document.createElement('div');
  grid.className = 'herd-grid';
  li.appendChild(grid);
  try {
    const resp = await fetch(`/api/files/${f.id}/monkes`);
    if (!resp.ok) throw 0;
    const idx = await resp.json();
    for (const m of idx.monkes) {
      const tile = document.createElement('div');
      tile.className = 'tile';
      tile.title = `click to inspect the hidden data in monke ${m.index + 1}`;
      const per = idx.per_monke_payload_bytes;
      const thisPayload = (m.index === idx.num_monkes - 1)
        ? (idx.size - per * (idx.num_monkes - 1))
        : per;
      tile.innerHTML = `
        <img src="${m.url}" alt="monke ${m.index + 1}" loading="lazy">
        <div class="tag">
          <span class="idx">#${String(m.index + 1).padStart(3, '0')}</span>
          &nbsp;·&nbsp;${fmtBytes(thisPayload)}
        </div>
      `;
      tile.addEventListener('click', () => openMonkeModal(idx, m, thisPayload));
      grid.appendChild(tile);
    }
  } catch {
    grid.innerHTML = '<div class="empty">could not load the herd</div>';
  }
}

// ------------------------------------------------------------------ LSB modal

async function openMonkeModal(idx, mref, payloadLen) {
  const dlg = $('#monke-modal');
  $('#monke-modal-title').textContent = `Monke #${String(mref.index + 1).padStart(3, '0')} of ${String(idx.num_monkes).padStart(3, '0')} — ${idx.name}`;
  $('#lsb-amp-bits').textContent = (256 / (1 << idx.lsb_bits)).toString();

  const byteOffset = mref.index * idx.per_monke_payload_bytes;
  $('#monke-modal-meta').innerHTML = `
    <div><span class="k">file_id</span>        <span class="v">${escHtml(idx.id)}</span></div>
    <div><span class="k">chunk_index</span>    <span class="v highlight">${mref.index} of ${idx.num_monkes}</span></div>
    <div><span class="k">lsb_bits</span>       <span class="v">${idx.lsb_bits} bit${idx.lsb_bits > 1 ? 's' : ''}/channel</span></div>
    <div><span class="k">payload</span>        <span class="v">${fmtBytes(payloadLen)} (bytes ${byteOffset}..${byteOffset + payloadLen - 1} of the original)</span></div>
    <div><span class="k">file_hash</span>      <span class="v">${escHtml(idx.file_hash)}</span></div>
  `;

  // Clear canvases
  for (const id of ['canvas-cover', 'canvas-shard', 'canvas-lsb']) {
    const c = $('#' + id);
    const ctx = c.getContext('2d');
    ctx.clearRect(0, 0, c.width, c.height);
  }

  dlg.showModal();

  try {
    const [coverImgData, shardImgData] = await Promise.all([
      loadImageData('/static/monke.png'),
      loadImageData(mref.url),
    ]);
    drawImageData($('#canvas-cover'), coverImgData);
    drawImageData($('#canvas-shard'), shardImgData);

    const lsb = buildLsbView(shardImgData, coverImgData, idx.lsb_bits);
    drawImageData($('#canvas-lsb'), lsb);
  } catch (e) {
    console.error('lsb render failed', e);
  }
}

function loadImageData(url) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => {
      const c = document.createElement('canvas');
      c.width = img.naturalWidth;
      c.height = img.naturalHeight;
      const ctx = c.getContext('2d', { willReadFrequently: true });
      ctx.drawImage(img, 0, 0);
      resolve(ctx.getImageData(0, 0, c.width, c.height));
    };
    img.onerror = reject;
    img.src = url;
  });
}

function drawImageData(canvas, data) {
  canvas.width = data.width;
  canvas.height = data.height;
  canvas.getContext('2d').putImageData(data, 0, 0);
}

/**
 * Build a visualization where the low `bits` bits of every channel in
 * `shard` are amplified. We XOR with the cover to isolate modifications,
 * then left-shift so the diff is maximally visible.
 */
function buildLsbView(shard, cover, bits) {
  const s = shard.data, c = cover.data;
  const out = new Uint8ClampedArray(s.length);
  const mask = (1 << bits) - 1;
  const shift = 8 - bits;
  for (let i = 0; i < s.length; i += 4) {
    const dr = (s[i]     ^ c[i])     & mask;
    const dg = (s[i + 1] ^ c[i + 1]) & mask;
    const db = (s[i + 2] ^ c[i + 2]) & mask;
    const da = (s[i + 3] ^ c[i + 3]) & mask;
    // Amplify and force opacity so we can see it
    out[i]     = dr << shift;
    out[i + 1] = dg << shift;
    out[i + 2] = db << shift;
    out[i + 3] = 255;
    // If alpha channel carried data, fold it into brightness
    if (da) {
      const a = da << shift;
      out[i]     = Math.min(255, out[i]     + a / 3);
      out[i + 1] = Math.min(255, out[i + 1] + a / 3);
      out[i + 2] = Math.min(255, out[i + 2] + a / 3);
    }
  }
  return new ImageData(out, shard.width, shard.height);
}

function bindMonkeModal() {
  $('#monke-modal-close').onclick = () => $('#monke-modal').close();
  $('#monke-modal').addEventListener('click', (e) => {
    if (e.target.tagName === 'DIALOG') $('#monke-modal').close();
  });
}

// ------------------------------------------------------------------ player

function openPlayer(f) {
  const dlg = $('#player');
  const v = $('#player-video');
  $('#player-title').textContent = `The Monkes Are Projecting — ${f.name}`;
  v.src = `/api/files/${f.id}/stream`;
  v.load();
  dlg.showModal();
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

// ------------------------------------------------------------------ upload

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
  dropArea.addEventListener('dragover', (e) => { e.preventDefault(); dropArea.classList.add('drag'); });
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
          if (ev.lengthComputable) bar.style.width = `${Math.round((ev.loaded / ev.total) * 100)}%`;
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
      const d = await (await fetch('/api/files')).json();
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

// ------------------------------------------------------------------ activity + peers (SSE)

const ACTIVITY_MAX = 100;
const activity = [];

function iconFor(kind) {
  return {
    upload: '⬆',
    download: '⬇',
    stream: '►',
    shard_served: '○',
    delete: '✕',
    index_listed: '☰',
    index_ready: '✓',
  }[kind] ?? '·';
}

function describeEvent(ev) {
  const peer = ev.peer ? `<span class="peer">${escHtml(ev.peer)}</span>` : 'someone';
  const name = ev.name ? `<span class="name">${escHtml(ev.name)}</span>` : '';
  const mo = (n, t) => `<span class="monke">monke ${n + 1} of ${t}</span>`;
  switch (ev.kind) {
    case 'upload':
      return `${peer} summoned ${name} as ${ev.num_monkes} ${monkeWord(ev.num_monkes)} (${fmtBytes(ev.size)})`;
    case 'download':
      return `${peer} downloaded ${name} (${fmtBytes(ev.size)})`;
    case 'stream':
      return `${peer} streamed ${name} bytes ${ev.range_start}–${ev.range_end_inclusive} (${fmtBytes(ev.slice_len)})`;
    case 'shard_served':
      return `${peer} pulled ${mo(ev.chunk_index, ev.total)} of ${name}`;
    case 'delete':
      return `${peer} returned ${name} to the wild`;
    case 'index_listed':
      return `${peer} discovered ${name} (${ev.num_monkes} ${monkeWord(ev.num_monkes)})`;
    case 'index_ready':
      return `index ready — ${ev.files} file${ev.files === 1 ? '' : 's'}, ${ev.monkes} ${monkeWord(ev.monkes)}`;
    default:
      return `<span class="name">${escHtml(ev.kind)}</span>`;
  }
}

function classFor(kind) {
  return {
    upload: 'upload',
    download: 'download',
    stream: 'stream',
    shard_served: 'shard',
    index_listed: 'shard',
    delete: 'delete',
    index_ready: 'index',
  }[kind] ?? 'index';
}

function renderActivity() {
  const list = $('#activity-list');
  if (!activity.length) {
    list.innerHTML = '<li class="empty">waiting for monke chatter…</li>';
    return;
  }
  list.innerHTML = activity.slice().reverse().map((ev) => `
    <li class="${classFor(ev.kind)}">
      <span class="ev-time">${fmtTime(ev.ts)}</span>
      <span class="ev-body">
        <span class="ev-icon">${iconFor(ev.kind)}</span>
        ${describeEvent(ev)}
      </span>
    </li>
  `).join('');
}

function pushEvent(ev) {
  activity.push(ev);
  if (activity.length > ACTIVITY_MAX) activity.splice(0, activity.length - ACTIVITY_MAX);
  renderActivity();
  // Some events should also refresh data:
  if (['upload', 'delete'].includes(ev.kind)) {
    Promise.all([refreshList(), refreshStats(), refreshPeers()]).catch(() => {});
  } else if (['download', 'stream', 'shard_served', 'index_listed'].includes(ev.kind)) {
    refreshPeers().catch(() => {});
  }
}

async function refreshPeers() {
  try {
    const r = await fetch('/api/peers');
    if (!r.ok) throw 0;
    const peers = await r.json();
    const list = $('#peer-list');
    if (!peers.length) {
      list.innerHTML = '<li class="empty">no peers yet</li>';
      return;
    }
    list.innerHTML = peers.map((p) => `
      <li>
        <div><span class="ip">${escHtml(p.ip)}</span> <span class="stat-n">· ${p.events} event${p.events === 1 ? '' : 's'}</span></div>
        <div class="time">last ${fmtRelative(p.last_seen)}</div>
      </li>
    `).join('');
  } catch {}
}

function connectEventStream() {
  const dot = $('#live-dot');
  const es = new EventSource('/api/events');
  es.onopen = () => { dot.classList.add('connected'); };
  es.onmessage = (msg) => {
    try {
      const ev = JSON.parse(msg.data);
      pushEvent(ev);
    } catch {}
  };
  es.onerror = () => {
    dot.classList.remove('connected');
    // EventSource auto-reconnects
  };
}

// ------------------------------------------------------------------ toast

function toast(msg, kind = '') {
  const el = document.createElement('div');
  el.className = `toast ${kind}`.trim();
  el.textContent = msg;
  document.body.appendChild(el);
  setTimeout(() => el.remove(), 3500);
}

// ------------------------------------------------------------------

document.addEventListener('DOMContentLoaded', () => {
  bindUpload();
  bindPlayer();
  bindMonkeModal();
  $('#refresh-btn').onclick = () => Promise.all([refreshList(), refreshStats(), refreshPeers()]);
  $('#clear-activity').onclick = () => { activity.length = 0; renderActivity(); };
  refreshStats();
  refreshList();
  refreshPeers();
  connectEventStream();
  setInterval(refreshStats, 15000);
});
