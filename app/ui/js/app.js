import { call, listen, hasBackend, openFileDialog, saveFileDialog } from './api.js';
import { MapView, BASEMAPS } from './map.js';
import { ProfileChart, drawPolar } from './charts.js';
import {
  newProject, newSite, newLink, defaultLegend, NLCD_CLASSES, DEFAULT_CLUTTER_HEIGHTS,
  CLIMATES, GROUNDS, RAIN_ZONES, DEM_RESOLUTIONS,
} from './model.js';

// ---------------------------------------------------------------- state ----
const AUTOSAVE_KEY = 'pathless.autosave';
const state = {
  project: loadAutosave() || newProject(),
  tab: 'project',
  mode: 'none',
  selectedSite: null,
  selectedLink: null,
  pendingLinkSite: null,
  catalog: [],
  dataStatus: null,
  plan: null,
  linkReports: {},
  linkStatus: {},
  coverageResult: null,
  antennaPreview: { id: null, freq: 900 },
  projectPath: null,
};

function loadAutosave() {
  try {
    const raw = localStorage.getItem(AUTOSAVE_KEY);
    if (!raw) return null;
    return migrate(JSON.parse(raw));
  } catch { return null; }
}
function autosave() {
  try { localStorage.setItem(AUTOSAVE_KEY, JSON.stringify(state.project)); } catch { /* storage unavailable */ }
}
function migrate(p) {
  const base = newProject();
  return {
    ...base, ...p,
    data: { ...base.data, ...(p.data || {}) },
    env: { ...base.env, ...(p.env || {}), clutterHeights: { ...DEFAULT_CLUTTER_HEIGHTS, ...(p.env?.clutterHeights || {}) } },
    coverage: { ...base.coverage, ...(p.coverage || {}) },
  };
}
function changed({ map = true, panel = false } = {}) {
  autosave();
  if (map) renderMap();
  if (panel) renderPanel();
}

// --------------------------------------------------------------- helpers ---
const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const fmt = (v, d = 1) => (v == null || Number.isNaN(v) ? '—' : Number(v).toFixed(d));
const getPath = (o, path) => path.split('.').reduce((a, k) => (a == null ? a : a[k]), o);
function setPath(o, path, v) {
  const ks = path.split('.');
  const last = ks.pop();
  const t = ks.reduce((a, k) => (a[k] ??= {}), o);
  t[last] = v;
}
const site = (id) => state.project.sites.find((s) => s.id === id);
const link = (id) => state.project.links.find((l) => l.id === id);
const dbmToW = (dbm) => 10 ** ((dbm - 30) / 10);

function toast(msg, kind = '') {
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.textContent = msg;
  $('#toasts').appendChild(el);
  setTimeout(() => el.remove(), kind === 'error' ? 9000 : 4500);
}

function busy(title) {
  $('#busy-title').textContent = title;
  $('#busy-bar').style.width = '0%';
  $('#busy-msg').textContent = '';
  $('#busy').classList.remove('hidden');
}
function unbusy() { $('#busy').classList.add('hidden'); }

listen('progress', (p) => {
  if (p.total > 0) $('#busy-bar').style.width = `${Math.min(100, (100 * p.done) / p.total)}%`;
  $('#busy-msg').textContent = p.message || `${p.stage} ${p.done}/${p.total}`;
});

/** Two-way bind every [data-bind] control in `root` to properties of `obj`. */
function bind(root, obj, onChange) {
  for (const el of $$('[data-bind]', root)) {
    const key = el.dataset.bind;
    const kind = el.dataset.kind || (el.type === 'number' ? 'num' : el.type === 'checkbox' ? 'bool' : 'str');
    const v = getPath(obj, key);
    if (kind === 'bool') el.checked = !!v; else el.value = v ?? '';
    el.addEventListener('change', () => {
      let nv = el.value;
      if (kind === 'num') nv = el.value === '' ? null : parseFloat(el.value);
      if (kind === 'bool') nv = el.checked;
      setPath(obj, key, nv);
      onChange?.(key, nv);
    });
  }
}

const row = (label, control, unit = '') => `<div class="row"><label>${label}</label>${control}${unit ? `<span class="unit">${unit}</span>` : ''}</div>`;
const num = (bindKey, step = 'any', extra = '') => `<input type="number" step="${step}" data-bind="${bindKey}" ${extra}>`;
const opts = (pairs, cur) => pairs.map(([v, l]) => `<option value="${esc(v)}" ${v === cur ? 'selected' : ''}>${esc(l)}</option>`).join('');

function antennaOptions(selected) {
  const groups = {};
  for (const a of state.catalog) (groups[a.category || 'Other'] ??= []).push(a);
  return `<option value="">— Isotropic (0 dBi) —</option>` + Object.entries(groups).map(([g, list]) =>
    `<optgroup label="${esc(g)}">${list.map((a) => `<option value="${esc(a.id)}" ${a.id === selected ? 'selected' : ''}>${esc(a.name)}</option>`).join('')}</optgroup>`).join('');
}

// ------------------------------------------------------------------ map ----
const mapView = new MapView($('#map'), {
  onMapClick: (ll) => {
    if (state.mode === 'addSite') {
      const s = newSite(ll.lat, ll.lng, state.project.sites.length);
      if (state.catalog.length) s.antennaId = state.catalog.find((a) => a.category === 'Omni')?.id || null;
      state.project.sites.push(s);
      state.selectedSite = s.id;
      setMode('none');
      switchTab('sites');
      changed();
      refreshGroundElevations();
    }
  },
  onSiteClick: (id) => {
    if (state.mode === 'addLink') {
      if (!state.pendingLinkSite) {
        state.pendingLinkSite = id;
        showHint(`Link from ${site(id).name}: now click the second site`);
      } else if (state.pendingLinkSite !== id) {
        const l = newLink(state.pendingLinkSite, id, state.project.links.length);
        l.name = `${site(state.pendingLinkSite).name} ↔ ${site(id).name}`;
        state.project.links.push(l);
        state.selectedLink = l.id;
        setMode('none');
        switchTab('links');
        changed();
      }
      return;
    }
    state.selectedSite = id;
    switchTab('sites');
    renderMap();
  },
  onSiteMoved: (id, lat, lon) => {
    const s = site(id);
    s.lat = lat; s.lon = lon;
    invalidateLinks(id);
    changed({ panel: state.tab === 'sites' });
    refreshGroundElevations();
  },
  onLinkClick: (id) => {
    if (state.mode !== 'none') return;
    state.selectedLink = id;
    switchTab('links');
    renderMap();
    if (state.linkReports[id]) openDrawer(state.linkReports[id]);
  },
  onAreaChanged: (pts, fresh) => {
    state.project.area = pts;
    state.plan = null;
    if (fresh) setMode('none');
    changed({ panel: state.tab === 'project' });
    if (fresh) switchTab('project');
  },
  onMouseMove: (ll) => updateReadout(ll),
});

let readoutTimer = null;
function updateReadout(ll) {
  const base = `${ll.lat.toFixed(5)}, ${ll.lng.toFixed(5)}`;
  $('#cursor-readout').textContent = base;
  if (!hasBackend() || !state.dataStatus) return;
  clearTimeout(readoutTimer);
  readoutTimer = setTimeout(async () => {
    try {
      const [elev, cov] = await Promise.all([
        call('elevation_at', { lat: ll.lat, lon: ll.lng }),
        state.coverageResult ? call('coverage_value_at', { lat: ll.lat, lon: ll.lng }) : null,
      ]);
      let txt = base;
      if (elev != null) txt += ` · ${elev.toFixed(1)} m`;
      if (cov != null) txt += ` · ${cov.toFixed(1)} ${state.coverageResult.unit}`;
      $('#cursor-readout').textContent = txt;
    } catch { /* outside data */ }
  }, 60);
}

function renderMap() {
  mapView.renderArea(state.project.area);
  mapView.renderSites(state.project.sites, state.selectedSite);
  mapView.renderLinks(state.project.links, state.project.sites, state.selectedLink, state.linkStatus);
}

function showHint(text) {
  const el = $('#mode-hint');
  el.textContent = text;
  el.classList.toggle('hidden', !text);
}

function setMode(mode) {
  if (state.mode === mode) mode = 'none';
  state.mode = mode;
  state.pendingLinkSite = null;
  mapView.setMode(mode);
  $$('#toolbar [data-mode]').forEach((b) => b.classList.toggle('active', b.dataset.mode === mode));
  showHint({
    drawArea: 'Click to add points · double-click / Enter to finish · Backspace undo · Esc cancel',
    addSite: 'Click the map to place a site · Esc to cancel',
    addLink: state.project.sites.length < 2 ? 'Add at least two sites first' : 'Click the first site of the link',
  }[mode] || '');
}

function invalidateLinks(siteId) {
  for (const l of state.project.links) {
    if (!siteId || l.siteA === siteId || l.siteB === siteId) {
      delete state.linkReports[l.id];
      delete state.linkStatus[l.id];
    }
  }
}

async function refreshGroundElevations() {
  if (!hasBackend() || !state.dataStatus) return;
  for (const s of state.project.sites) {
    try { s._groundM = await call('elevation_at', { lat: s.lat, lon: s.lon }); } catch { s._groundM = null; }
  }
  if (state.tab === 'sites') renderPanel();
}

// --------------------------------------------------------------- panels ----
function switchTab(tab) {
  state.tab = tab;
  $$('#tabs button').forEach((b) => b.classList.toggle('active', b.dataset.tab === tab));
  renderPanel();
}

function renderPanel() {
  const panel = $('#panel');
  ({
    project: renderProjectPanel,
    sites: renderSitesPanel,
    links: renderLinksPanel,
    coverage: renderCoveragePanel,
    antennas: renderAntennasPanel,
    environment: renderEnvironmentPanel,
  }[state.tab])(panel);
}

function areaInfo() {
  const a = state.project.area;
  if (a.length < 3) return null;
  const lons = a.map((p) => p[0]), lats = a.map((p) => p[1]);
  const bbox = [Math.min(...lons), Math.min(...lats), Math.max(...lons), Math.max(...lats)];
  // planar shoelace on a local equirectangular projection — good enough for display
  const lat0 = (bbox[1] + bbox[3]) / 2 * Math.PI / 180, R = 6371;
  let s = 0;
  for (let i = 0; i < a.length; i++) {
    const [x1, y1] = a[i], [x2, y2] = a[(i + 1) % a.length];
    s += (x1 * Math.cos(lat0)) * y2 - (x2 * Math.cos(lat0)) * y1;
  }
  const km2 = Math.abs(s / 2) * (Math.PI / 180 * R) ** 2;
  return { bbox, km2 };
}

function fmtBytes(b) {
  if (b == null) return '—';
  const u = ['B', 'KB', 'MB', 'GB'];
  let i = 0;
  while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
  return `${b.toFixed(i ? 1 : 0)} ${u[i]}`;
}

function renderProjectPanel(panel) {
  const p = state.project;
  const ai = areaInfo();
  const ds = state.dataStatus;
  const plan = state.plan;
  panel.innerHTML = `
    <h3>Study area</h3>
    <div class="card">
      ${ai ? `
        <table class="kv">
          <tr><td>Vertices</td><td class="num">${p.area.length}</td></tr>
          <tr><td>Area</td><td class="num">${fmt(ai.km2, 1)} km²</td></tr>
          <tr><td>Bounds</td><td class="num">${ai.bbox.map((v) => v.toFixed(4)).join(', ')}</td></tr>
        </table>
        <div class="btnrow">
          <button id="zoom-area">Zoom to area</button>
          <button id="redraw-area">Redraw</button>
          <button id="clear-area" class="danger">Clear</button>
        </div>` : `
        <p class="note">Use <b>▱ Draw area</b> to outline where you want analysis. PathLess works out which
        USGS 3DEP elevation and NLCD land-cover tiles cover it and downloads them for you.
        Sites outside the polygon are included automatically.</p>
        <button id="redraw-area" class="primary">▱ Draw area</button>`}
    </div>
    <h3>Terrain &amp; clutter data</h3>
    <div class="card">
      ${row('Elevation resolution', `<select data-bind="data.demResolution">${opts(DEM_RESOLUTIONS, p.data.demResolution)}</select>`)}
      ${row('Use NLCD clutter', '<input type="checkbox" data-bind="data.useClutter">')}
      <div class="btnrow">
        <button id="plan-data">Check what's needed</button>
        <button id="fetch-data" class="primary">⇩ Fetch data</button>
      </div>
      ${plan ? `
        <h4>Download plan</h4>
        <table class="kv">
          <tr><td>Elevation tiles</td><td class="num">${plan.elevation.tileCount} (${plan.elevation.cached} cached)</td></tr>
          <tr><td>Elevation pixels</td><td class="num">${(plan.elevation.totalPixels / 1e6).toFixed(1)} M</td></tr>
          <tr><td>Est. download</td><td class="num">${fmtBytes(plan.elevation.estimatedBytes + (plan.landcover?.estimatedBytes || 0))}</td></tr>
          ${plan.landcover ? `<tr><td>Land-cover tiles</td><td class="num">${plan.landcover.tileCount} (${plan.landcover.cached} cached)</td></tr>` : ''}
        </table>
        ${(plan.warnings || []).map((w) => `<p class="note">⚠ ${esc(w)}</p>`).join('')}` : ''}
      ${ds ? `
        <h4>Loaded</h4>
        <table class="kv">
          <tr><td>Elevation</td><td class="num">${ds.elevation.width} × ${ds.elevation.height} px</td></tr>
          <tr><td>Pixel size</td><td class="num">~${fmt(ds.elevation.pixelM, 1)} m</td></tr>
          <tr><td>Elevation range</td><td class="num">${fmt(ds.elevation.minM, 0)} – ${fmt(ds.elevation.maxM, 0)} m</td></tr>
          ${ds.landcover ? `<tr><td>Land cover</td><td class="num">${ds.landcover.width} × ${ds.landcover.height} px</td></tr>` : ''}
        </table>` : '<p class="note">No terrain loaded yet.</p>'}
    </div>
    <h3>Cache</h3>
    <div class="card">
      <div id="cache-info" class="note">…</div>
      <div class="btnrow"><button id="clear-cache" class="danger small">Clear download cache</button></div>
    </div>
    <h3>Workflow</h3>
    <p class="note">1. Draw area → 2. Fetch data → 3. Add sites (set antennas &amp; radios) →
    4. Add links and analyze path profiles, or 5. Run coverage for a site. Environment settings
    (climate, ground, reliability, clutter heights) live under <b>Environment</b>.</p>`;
  bind(panel, p, () => { state.plan = null; changed({ map: false }); });
  $('#redraw-area', panel)?.addEventListener('click', () => setMode('drawArea'));
  $('#zoom-area', panel)?.addEventListener('click', () => mapView.fitTo(p.area));
  $('#clear-area', panel)?.addEventListener('click', () => { p.area = []; state.plan = null; changed({ panel: true }); });
  $('#plan-data', panel).addEventListener('click', planData);
  $('#fetch-data', panel).addEventListener('click', fetchData);
  $('#clear-cache', panel).addEventListener('click', async () => {
    try { await call('clear_cache'); toast('Cache cleared', 'ok'); state.plan = null; renderPanel(); } catch (e) { toast(e.message, 'error'); }
  });
  if (hasBackend()) {
    call('cache_info').then((c) => { const el = $('#cache-info', panel); if (el) el.textContent = `${fmtBytes(c.bytes)} in ${c.dir}`; })
      .catch(() => {});
  }
}

function requireArea() {
  if (state.project.area.length >= 3 || state.project.sites.length) return true;
  toast('Draw a study area (or add sites) first.', 'error');
  return false;
}

async function planData() {
  if (!requireArea()) return;
  try {
    state.plan = await call('plan_data', { project: state.project });
    renderPanel();
  } catch (e) { toast(e.message, 'error'); }
}

async function fetchData() {
  if (!requireArea()) return;
  busy('Fetching terrain & clutter');
  try {
    state.dataStatus = await call('fetch_data', { project: state.project });
    state.plan = null;
    toast('Terrain data ready', 'ok');
    invalidateLinks();
    renderMap();
    await refreshGroundElevations();
  } catch (e) {
    toast(`Data fetch failed: ${e.message}`, 'error');
  } finally {
    unbusy();
    if (state.tab === 'project') renderPanel();
  }
}

function renderSitesPanel(panel) {
  const p = state.project;
  const s = site(state.selectedSite);
  panel.innerHTML = `
    <h3>Sites</h3>
    ${p.sites.length ? p.sites.map((x, i) => `
      <div class="list-item ${x.id === state.selectedSite ? 'selected' : ''}" data-id="${x.id}">
        <span class="dot" style="background:${x.color}"></span>
        <div class="grow"><div>${i + 1}. ${esc(x.name)}</div>
        <div class="sub">${x.lat.toFixed(5)}, ${x.lon.toFixed(5)} · ${x.antennaHeightM} m AGL · ${esc(state.catalog.find((a) => a.id === x.antennaId)?.name || 'Isotropic')}</div></div>
      </div>`).join('') : '<p class="note">No sites yet. Use <b>⊕ Add site</b> and click on the map.</p>'}
    <div class="btnrow"><button id="add-site">⊕ Add site</button></div>
    ${s ? `
      <h3 style="margin-top:16px">Edit site</h3>
      <div class="card" id="site-form">
        ${row('Name', '<input type="text" data-bind="name">')}
        ${row('Latitude', num('lat', '0.00001'), '°')}
        ${row('Longitude', num('lon', '0.00001'), '°')}
        ${row('Ground elevation', `<input type="text" disabled value="${s._groundM != null ? s._groundM.toFixed(1) : 'fetch data'}">`, 'm')}
        <h4>Antenna</h4>
        ${row('Antenna', `<select id="site-antenna">${antennaOptions(s.antennaId)}</select>`)}
        ${row('Height above ground', num('antennaHeightM', '0.5'), 'm')}
        ${row('Azimuth', num('azimuthDeg', '1'), '° T')}
        ${row('Mechanical downtilt', num('downtiltDeg', '0.5'), '°')}
        <h4>Radio</h4>
        ${row('Tx power', num('txPowerDbm', '0.5'), 'dBm')}
        <div class="row"><label></label><span class="note" id="tx-watts">${dbmToW(s.txPowerDbm).toPrecision(3)} W</span></div>
        ${row('Line loss (cable + connectors)', num('lineLossDb', '0.1'), 'dB')}
        ${row('Other losses', num('otherLossDb', '0.1'), 'dB')}
        ${row('Rx sensitivity', num('rxSensitivityDbm', '0.5'), 'dBm')}
        ${row('Color', '<input type="color" data-bind="color">')}
        <div class="btnrow">
          <button id="site-coverage" class="primary">Run coverage from here</button>
          <button id="site-delete" class="danger">Delete</button>
        </div>
      </div>` : ''}`;
  $$('.list-item', panel).forEach((el) => el.addEventListener('click', () => {
    state.selectedSite = el.dataset.id;
    const x = site(el.dataset.id);
    mapView.map.easeTo({ center: [x.lon, x.lat] });
    renderPanel(); renderMap();
  }));
  $('#add-site', panel).addEventListener('click', () => setMode('addSite'));
  if (!s) return;
  const form = $('#site-form', panel);
  bind(form, s, (key) => {
    if (key === 'txPowerDbm') $('#tx-watts', panel).textContent = `${dbmToW(s.txPowerDbm).toPrecision(3)} W`;
    if (key === 'lat' || key === 'lon') refreshGroundElevations();
    invalidateLinks(s.id);
    changed({ panel: key === 'name' || key === 'color' });
  });
  $('#site-antenna', panel).addEventListener('change', (e) => { s.antennaId = e.target.value || null; invalidateLinks(s.id); changed(); });
  $('#site-delete', panel).addEventListener('click', () => {
    p.links = p.links.filter((l) => l.siteA !== s.id && l.siteB !== s.id);
    p.sites = p.sites.filter((x) => x.id !== s.id);
    state.selectedSite = null;
    changed({ panel: true });
  });
  $('#site-coverage', panel).addEventListener('click', () => { p.coverage.siteId = s.id; switchTab('coverage'); });
}

function linkBadge(id) {
  const st = state.linkStatus[id];
  if (!st) return '<span class="badge">not analyzed</span>';
  const r = state.linkReports[id];
  const m = Math.min(r.budgetAB.fadeMarginDb, r.budgetBA.fadeMarginDb);
  return `<span class="badge ${st}">${fmt(m, 1)} dB margin</span>`;
}

function renderLinksPanel(panel) {
  const p = state.project;
  const l = link(state.selectedLink);
  const siteOpts = (cur) => p.sites.map((s) => `<option value="${s.id}" ${s.id === cur ? 'selected' : ''}>${esc(s.name)}</option>`).join('');
  panel.innerHTML = `
    <h3>Links</h3>
    ${p.links.length ? p.links.map((x) => `
      <div class="list-item ${x.id === state.selectedLink ? 'selected' : ''}" data-id="${x.id}">
        <div class="grow"><div>${esc(x.name)}</div>
        <div class="sub">${fmt(x.frequencyMhz, 0)} MHz · ${esc(site(x.siteA)?.name)} ↔ ${esc(site(x.siteB)?.name)}</div></div>
        ${linkBadge(x.id)}
      </div>`).join('') : '<p class="note">No links yet. Use <b>⟷ Add link</b> and click two sites.</p>'}
    <div class="btnrow">
      <button id="add-link">⟷ Add link</button>
      ${p.links.length ? '<button id="analyze-all">Analyze all</button>' : ''}
    </div>
    ${l ? `
      <h3 style="margin-top:16px">Edit link</h3>
      <div class="card" id="link-form">
        ${row('Name', '<input type="text" data-bind="name">')}
        ${row('Site A', `<select data-bind="siteA">${siteOpts(l.siteA)}</select>`)}
        ${row('Site B', `<select data-bind="siteB">${siteOpts(l.siteB)}</select>`)}
        ${row('Frequency', num('frequencyMhz', '0.001'), 'MHz')}
        ${row('Polarization', `<select data-bind="polarization">${opts([['vertical', 'Vertical'], ['horizontal', 'Horizontal']], l.polarization)}</select>`)}
        ${row('Antennas aimed at each other', '<input type="checkbox" data-bind="alignAntennas">')}
        <h4>Microwave availability (used ≥ 3 GHz)</h4>
        ${row('ITU rain zone', `<select data-bind="rainZone">${opts(RAIN_ZONES.map((z) => [z, `Zone ${z}`]), l.rainZone)}</select>`)}
        ${row('Custom R<sub>0.01</sub> (blank = zone)', num('customR001', '1'), 'mm/h')}
        ${row('Refractivity gradient dN1', num('dn1', '1'), 'N/km')}
        ${row('Target availability', num('targetAvailabilityPct', '0.001'), '%')}
        <div class="btnrow">
          <button id="link-analyze" class="primary">Analyze path</button>
          <button id="link-delete" class="danger">Delete</button>
        </div>
      </div>` : ''}`;
  $$('.list-item', panel).forEach((el) => el.addEventListener('click', () => {
    state.selectedLink = el.dataset.id;
    renderPanel(); renderMap();
    if (state.linkReports[el.dataset.id]) openDrawer(state.linkReports[el.dataset.id]);
  }));
  $('#add-link', panel).addEventListener('click', () => setMode('addLink'));
  $('#analyze-all', panel)?.addEventListener('click', async () => {
    for (const x of p.links) await analyzeLink(x.id, false);
    renderPanel();
  });
  if (!l) return;
  bind($('#link-form', panel), l, (key) => {
    delete state.linkReports[l.id]; delete state.linkStatus[l.id];
    changed({ panel: key === 'name' || key === 'siteA' || key === 'siteB' });
  });
  $('#link-analyze', panel).addEventListener('click', () => analyzeLink(l.id, true));
  $('#link-delete', panel).addEventListener('click', () => {
    p.links = p.links.filter((x) => x.id !== l.id);
    state.selectedLink = null;
    closeDrawer();
    changed({ panel: true });
  });
}

async function analyzeLink(id, open) {
  if (!state.dataStatus) { toast('Fetch terrain data first (Project tab).', 'error'); return; }
  try {
    const r = await call('analyze_link', { project: state.project, linkId: id });
    state.linkReports[id] = r;
    const m = Math.min(r.budgetAB.fadeMarginDb, r.budgetBA.fadeMarginDb);
    const avail = r.microwave ? r.microwave.meetsTarget : true;
    state.linkStatus[id] = m >= 10 && avail ? 'good' : m >= 0 ? 'warn' : 'bad';
    renderMap();
    if (state.tab === 'links') renderPanel();
    if (open) openDrawer(r);
  } catch (e) { toast(`${link(id)?.name}: ${e.message}`, 'error'); }
}

// --------------------------------------------------------------- drawer ----
const chart = new ProfileChart($('#profile-canvas'), $('#profile-tip'));
$('#drawer-close').addEventListener('click', closeDrawer);
function closeDrawer() { $('#drawer').classList.add('hidden'); }

function openDrawer(r) {
  $('#drawer').classList.remove('hidden');
  $('#drawer-title').textContent = `Path profile — ${r.siteA.name} ↔ ${r.siteB.name} · ${fmt(r.distanceKm, 2)} km · ${fmt(r.frequencyMhz, 1)} MHz`;
  chart.setData(r);
  const L = r.loss;
  const b = (x) => `
    <tr><td>EIRP</td><td class="num">${fmt(x.eirpDbm)} dBm</td></tr>
    <tr><td>Rx antenna gain</td><td class="num">${fmt(x.rxGainDbi)} dBi</td></tr>
    <tr><td>Rx level</td><td class="num">${fmt(x.rxLevelDbm)} dBm</td></tr>
    <tr><td>Field strength</td><td class="num">${fmt(x.fieldDbuvm)} dBµV/m</td></tr>
    <tr class="total"><td>Fade margin</td><td class="num" style="color:${x.fadeMarginDb >= 10 ? 'var(--good)' : x.fadeMarginDb >= 0 ? 'var(--warn)' : 'var(--bad)'}">${fmt(x.fadeMarginDb)} dB</td></tr>`;
  const mw = r.microwave;
  $('#link-results').innerHTML = `
    <div class="stats">
      <div class="stat"><div class="k">Path loss (${esc(r.model)})</div><div class="v">${fmt(L.totalDb)} dB</div></div>
      <div class="stat"><div class="k">Worst F1 clearance</div><div class="v">${fmt(r.clearance.worstRatio * 100, 0)}%</div></div>
      <div class="stat"><div class="k">Margin A→B</div><div class="v">${fmt(r.budgetAB.fadeMarginDb)} dB</div></div>
      <div class="stat"><div class="k">Margin B→A</div><div class="v">${fmt(r.budgetBA.fadeMarginDb)} dB</div></div>
    </div>
    <h4>Geometry</h4>
    <table class="kv">
      <tr><td></td><td class="num">${esc(r.siteA.name)}</td><td class="num">${esc(r.siteB.name)}</td></tr>
      <tr><td>Ground</td><td class="num">${fmt(r.siteA.groundM)} m</td><td class="num">${fmt(r.siteB.groundM)} m</td></tr>
      <tr><td>Antenna AMSL</td><td class="num">${fmt(r.siteA.antennaAmslM)} m</td><td class="num">${fmt(r.siteB.antennaAmslM)} m</td></tr>
      <tr><td>Azimuth</td><td class="num">${fmt(r.siteA.azimuthDeg, 2)}°</td><td class="num">${fmt(r.siteB.azimuthDeg, 2)}°</td></tr>
      <tr><td>Elevation angle</td><td class="num">${fmt(r.siteA.elevationDeg, 2)}°</td><td class="num">${fmt(r.siteB.elevationDeg, 2)}°</td></tr>
      <tr><td>Antenna gain to path</td><td class="num">${fmt(r.siteA.antennaGainDbi)} dBi</td><td class="num">${fmt(r.siteB.antennaGainDbi)} dBi</td></tr>
    </table>
    <h4>Path loss</h4>
    <table class="kv">
      <tr><td>Free space</td><td class="num">${fmt(L.fsplDb)} dB</td></tr>
      ${L.itmDb != null ? `<tr><td>Longley-Rice (ITM)${L.itmMode ? ` · ${esc(L.itmMode)}` : ''}</td><td class="num">${fmt(L.itmDb)} dB</td></tr>` : ''}
      ${L.diffractionDb != null ? `<tr><td>Diffraction (P.526 Δ-Bullington)</td><td class="num">${fmt(L.diffractionDb)} dB</td></tr>` : ''}
      <tr><td>Atmospheric gases (P.676)</td><td class="num">${fmt(L.gasDb, 2)} dB</td></tr>
      ${L.clutterDb ? `<tr><td>Terminal clutter (P.2108)</td><td class="num">${fmt(L.clutterDb)} dB</td></tr>` : ''}
      <tr class="total"><td>Total used</td><td class="num">${fmt(L.totalDb)} dB</td></tr>
    </table>
    <h4>Link budget A → B</h4><table class="kv">${b(r.budgetAB)}</table>
    <h4>Link budget B → A</h4><table class="kv">${b(r.budgetBA)}</table>
    ${mw ? `
      <h4>Microwave availability (ITU-R P.530)</h4>
      <table class="kv">
        <tr><td>Rain rate R<sub>0.01</sub></td><td class="num">${fmt(mw.r001, 0)} mm/h</td></tr>
        <tr><td>Rain fade exceeded 0.01%</td><td class="num">${fmt(mw.rainAtten001Db)} dB</td></tr>
        <tr><td>Multipath unavailability</td><td class="num">${mw.multipathUnavailPct.toExponential(2)} %</td></tr>
        <tr><td>Rain unavailability</td><td class="num">${mw.rainUnavailPct.toExponential(2)} %</td></tr>
        <tr class="total"><td>Availability</td><td class="num" style="color:${mw.meetsTarget ? 'var(--good)' : 'var(--bad)'}">${mw.availabilityPct.toFixed(5)} %</td></tr>
        <tr><td>Outage</td><td class="num">${fmt(mw.outageMinPerYear, 1)} min/yr</td></tr>
      </table>` : ''}
    ${(r.warnings || []).length ? `<h4>Notes</h4>${r.warnings.map((w) => `<p class="note">⚠ ${esc(w)}</p>`).join('')}` : ''}`;
}

// ------------------------------------------------------------- coverage ----
function renderCoveragePanel(panel) {
  const p = state.project;
  const c = p.coverage;
  if (!c.siteId && p.sites.length) c.siteId = p.sites[0].id;
  const units = { rxDbm: 'dBm', fieldDbuvm: 'dBµV/m', pathLossDb: 'dB' };
  const res = state.coverageResult;
  panel.innerHTML = `
    <h3>Area coverage</h3>
    ${p.sites.length ? '' : '<p class="note">Add a site first.</p>'}
    <div class="card" id="cov-form">
      ${row('Transmitting site', `<select data-bind="siteId">${p.sites.map((s) => `<option value="${s.id}">${esc(s.name)}</option>`).join('')}</select>`)}
      ${row('Frequency', num('frequencyMhz', '0.001'), 'MHz')}
      ${row('Polarization', `<select data-bind="polarization">${opts([['vertical', 'Vertical'], ['horizontal', 'Horizontal']], c.polarization)}</select>`)}
      <h4>Receiver</h4>
      ${row('Height above ground', num('rxHeightM', '0.5'), 'm')}
      ${row('Antenna gain', num('rxGainDbi', '0.5'), 'dBi')}
      ${row('Line loss', num('rxLineLossDb', '0.1'), 'dB')}
      <h4>Calculation</h4>
      ${row('Grid resolution', num('resolutionM', '1'), 'm')}
      ${row('Max radius', num('maxRadiusKm', '0.5'), 'km')}
      ${row('Display', `<select data-bind="output">${opts([['rxDbm', 'Received level (dBm)'], ['fieldDbuvm', 'Field strength (dBµV/m)'], ['pathLossDb', 'Path loss (dB)']], c.output)}</select>`)}
      <h4>Legend (${units[c.output]})</h4>
      <div id="legend-edit">${c.legend.map((e, i) => `
        <div class="row"><input type="color" value="${e.color}" data-li="${i}" data-f="color" style="flex:0 0 40px">
        <input type="number" step="1" value="${e.value}" data-li="${i}" data-f="value">
        <button class="small" data-del="${i}">✕</button></div>`).join('')}</div>
      <div class="btnrow">
        <button class="small" id="legend-add">+ Level</button>
        <button class="small" id="legend-reset">Reset</button>
      </div>
      <div class="btnrow">
        <button id="run-coverage" class="primary" ${p.sites.length ? '' : 'disabled'}>▶ Run coverage</button>
        ${res ? '<button id="clear-coverage">Clear</button>' : ''}
      </div>
    </div>
    ${res ? `
      <h3>Result</h3>
      <div class="stats">
        <div class="stat"><div class="k">Grid</div><div class="v">${res.width} × ${res.height}</div></div>
        <div class="stat"><div class="k">Compute time</div><div class="v">${fmt(res.computeMs / 1000, 1)} s</div></div>
        <div class="stat"><div class="k">Area above lowest level</div><div class="v">${fmt(res.coveredPct, 1)}%</div></div>
        <div class="stat"><div class="k">Model</div><div class="v" style="font-size:12px">${esc(res.model)}</div></div>
      </div>` : ''}
    <p class="note">Coverage is computed point-to-point (terrain profile from the site to every grid cell), within
    the study polygon when one is drawn, otherwise within the max radius. Hover the map to read values.</p>`;
  const form = $('#cov-form', panel);
  bind(form, c, (key) => {
    if (key === 'output') { c.legend = defaultLegend(c.output); changed({ map: false, panel: true }); return; }
    changed({ map: false });
  });
  $$('#legend-edit [data-li]', panel).forEach((el) => el.addEventListener('change', () => {
    const e = c.legend[+el.dataset.li];
    e[el.dataset.f] = el.dataset.f === 'value' ? parseFloat(el.value) : el.value;
    changed({ map: false });
  }));
  $$('#legend-edit [data-del]', panel).forEach((el) => el.addEventListener('click', () => {
    c.legend.splice(+el.dataset.del, 1); changed({ map: false, panel: true });
  }));
  $('#legend-add', panel).addEventListener('click', () => {
    const last = c.legend[c.legend.length - 1];
    c.legend.push({ value: last ? last.value + (c.output === 'pathLossDb' ? 10 : -10) : -100, color: '#888888' });
    changed({ map: false, panel: true });
  });
  $('#legend-reset', panel).addEventListener('click', () => { c.legend = defaultLegend(c.output); changed({ map: false, panel: true }); });
  $('#run-coverage', panel).addEventListener('click', runCoverage);
  $('#clear-coverage', panel)?.addEventListener('click', () => {
    state.coverageResult = null; mapView.clearCoverage(); renderLegend(); renderPanel();
  });
}

async function runCoverage() {
  if (!state.dataStatus) { toast('Fetch terrain data first (Project tab).', 'error'); return; }
  busy('Computing coverage');
  try {
    const r = await call('run_coverage', { project: state.project });
    state.coverageResult = r;
    mapView.showCoverage(r.imageDataUrl, r.bounds, $('#ov-opacity').value / 100);
    $('#ov-coverage').checked = true;
    renderLegend();
    toast(`Coverage done in ${(r.computeMs / 1000).toFixed(1)} s`, 'ok');
  } catch (e) {
    toast(`Coverage failed: ${e.message}`, 'error');
  } finally {
    unbusy();
    if (state.tab === 'coverage') renderPanel();
  }
}

function renderLegend() {
  const el = $('#legend');
  const r = state.coverageResult;
  if (!r) { el.classList.add('hidden'); return; }
  el.classList.remove('hidden');
  const higherIsBetter = r.output !== 'pathLossDb';
  el.innerHTML = `<b>${esc(site(r.siteId)?.name || '')}</b> · ${fmt(r.frequencyMhz, 1)} MHz<br>` +
    r.legend.map((e) => `<div class="lg-row"><span class="sw" style="background:${e.color}"></span>${higherIsBetter ? '≥' : '≤'} ${e.value} ${esc(r.unit)}</div>`).join('');
}

// ------------------------------------------------------------- antennas ----
function renderAntennasPanel(panel) {
  const prev = state.antennaPreview;
  if (!prev.id && state.catalog.length) prev.id = state.catalog[0].id;
  const cats = [...new Set(state.catalog.map((a) => a.category))];
  const filter = prev.filter || '';
  const list = state.catalog.filter((a) => !filter || a.category === filter);
  const sel = state.catalog.find((a) => a.id === prev.id);
  panel.innerHTML = `
    <h3>Antenna library</h3>
    <div class="row"><label>Category</label><select id="ant-filter"><option value="">All (${state.catalog.length})</option>${cats.map((c) => `<option ${c === filter ? 'selected' : ''}>${esc(c)}</option>`).join('')}</select></div>
    <div style="max-height:260px; overflow-y:auto; margin-bottom:10px">
      ${list.map((a) => `<div class="list-item ${a.id === prev.id ? 'selected' : ''}" data-id="${esc(a.id)}">
        <div class="grow"><div>${esc(a.name)}</div><div class="sub">${esc(a.category)} · ${fmt(a.minMhz, 0)}–${fmt(a.maxMhz, 0)} MHz${a.source === 'user' ? ' · imported' : ''}</div></div></div>`).join('')}
    </div>
    <div class="btnrow"><button id="ant-import">Import MSI / Planet file…</button></div>
    ${sel ? `
      <h3 style="margin-top:14px">${esc(sel.name)}</h3>
      <div class="card">
        ${row('Preview frequency', `<input type="number" id="ant-freq" value="${prev.freq}" step="1">`, 'MHz')}
        <canvas class="polar" id="ant-polar"></canvas>
        <table class="kv" id="ant-info" style="margin-top:8px"></table>
        ${sel.notes ? `<p class="note">${esc(sel.notes)}</p>` : ''}
        ${state.selectedSite ? `<div class="btnrow"><button id="ant-assign">Use on ${esc(site(state.selectedSite)?.name)}</button></div>` : ''}
      </div>` : ''}`;
  $('#ant-filter', panel).addEventListener('change', (e) => { prev.filter = e.target.value; renderPanel(); });
  $$('.list-item', panel).forEach((el) => el.addEventListener('click', () => {
    prev.id = el.dataset.id;
    const a = state.catalog.find((x) => x.id === prev.id);
    if (a && (prev.freq < a.minMhz || prev.freq > a.maxMhz)) prev.freq = Math.round(Math.sqrt(a.minMhz * a.maxMhz));
    renderPanel();
  }));
  $('#ant-import', panel).addEventListener('click', importAntenna);
  $('#ant-assign', panel)?.addEventListener('click', () => {
    const s = site(state.selectedSite); s.antennaId = prev.id; invalidateLinks(s.id); changed(); toast(`Antenna assigned to ${s.name}`, 'ok');
  });
  if (!sel) return;
  $('#ant-freq', panel).addEventListener('change', (e) => { prev.freq = parseFloat(e.target.value) || prev.freq; loadPattern(); });
  loadPattern();
}

async function loadPattern() {
  const prev = state.antennaPreview;
  if (!hasBackend()) return;
  try {
    const pat = await call('antenna_pattern', { id: prev.id, frequencyMhz: prev.freq });
    const canvas = $('#ant-polar');
    if (!canvas) return;
    drawPolar(canvas, pat);
    $('#ant-info').innerHTML = `
      <tr><td>Gain</td><td class="num">${fmt(pat.gain_dbi, 1)} dBi</td></tr>
      <tr><td>H beamwidth</td><td class="num">${fmt(pat.h_beamwidth_deg, 1)}°</td></tr>
      <tr><td>V beamwidth</td><td class="num">${fmt(pat.v_beamwidth_deg, 1)}°</td></tr>
      <tr><td>Front-to-back</td><td class="num">${fmt(pat.front_to_back_db, 1)} dB</td></tr>
      ${pat.electrical_tilt_deg ? `<tr><td>Electrical tilt</td><td class="num">${fmt(pat.electrical_tilt_deg, 1)}°</td></tr>` : ''}`;
  } catch (e) { toast(e.message, 'error'); }
}

async function importAntenna() {
  try {
    const path = await openFileDialog({ multiple: false, filters: [{ name: 'Antenna patterns', extensions: ['msi', 'pln', 'txt', 'ant'] }] });
    if (!path) return;
    const spec = await call('import_antenna', { path });
    await loadCatalog();
    state.antennaPreview.id = spec.id;
    toast(`Imported ${spec.name}`, 'ok');
    renderPanel();
  } catch (e) { toast(`Import failed: ${e.message}`, 'error'); }
}

async function loadCatalog() {
  if (!hasBackend()) return;
  try { state.catalog = await call('antenna_catalog'); } catch (e) { toast(e.message, 'error'); }
}

// ---------------------------------------------------------- environment ----
function renderEnvironmentPanel(panel) {
  const env = state.project.env;
  panel.innerHTML = `
    <h3>Propagation model</h3>
    <div class="card" id="env-form">
      ${row('Model', `<select data-bind="model">${opts([
        ['auto', 'Auto (ITM; microwave method for links ≥ 3 GHz)'],
        ['itm', 'Longley-Rice ITM'],
        ['microwave', 'Free space + P.526 diffraction'],
      ], env.model)}</select>`)}
      ${row('Climate', `<select data-bind="climate">${opts(CLIMATES, env.climate)}</select>`)}
      ${row('Surface refractivity N₀', num('n0', '1'), 'N')}
      ${row('k-factor (profile display, diffraction)', num('kFactor', '0.01'))}
      ${row('Ground', `<select data-bind="ground">${opts(Object.entries(GROUNDS).map(([k, v]) => [k, v[0]]), env.ground)}</select>`)}
      ${row('Permittivity ε', num('epsilon', '0.1', env.ground !== 'custom' ? 'disabled' : ''))}
      ${row('Conductivity σ', num('sigma', '0.001', env.ground !== 'custom' ? 'disabled' : ''), 'S/m')}
      <h4>Reliability (ITM)</h4>
      ${row('Variability mode', `<select data-bind="variability">${opts([['singleMessage', 'Single message'], ['accidental', 'Accidental'], ['mobile', 'Mobile'], ['broadcast', 'Broadcast']], env.variability)}</select>`)}
      ${row('Time', num('timePct', '1'), '%')}
      ${row('Location', num('locationPct', '1'), '%')}
      ${row('Situation (confidence)', num('situationPct', '1'), '%')}
      <h4>Clutter</h4>
      ${row('Terminal clutter loss (ITU-R P.2108)', '<input type="checkbox" data-bind="terminalClutterLoss">')}
      <p class="note">Heights (m) added on top of bare-earth terrain along the path, by NLCD class. Defaults are representative; tune for your region.</p>
      <div class="clutter-grid">${NLCD_CLASSES.map(([code, name, color]) => `
        <span class="dot" style="background:${color}"></span><span>${code} ${esc(name)}</span>
        <input type="number" step="0.5" data-bind="clutterHeights.${code}">`).join('')}</div>
      <div class="btnrow"><button class="small" id="clutter-reset">Reset clutter heights</button></div>
    </div>`;
  bind($('#env-form', panel), env, (key, v) => {
    if (key === 'ground' && v !== 'custom') { env.epsilon = GROUNDS[v][1]; env.sigma = GROUNDS[v][2]; }
    invalidateLinks();
    changed({ map: false, panel: key === 'ground' });
  });
  $('#clutter-reset', panel).addEventListener('click', () => { env.clutterHeights = { ...DEFAULT_CLUTTER_HEIGHTS }; changed({ map: false, panel: true }); });
}

// ------------------------------------------------------- file handling ----
async function saveProject() {
  try {
    let path = state.projectPath;
    if (!path) path = await saveFileDialog({ defaultPath: `${state.project.name}.pathless.json`, filters: [{ name: 'PathLess project', extensions: ['json'] }] });
    if (!path) return;
    await call('save_project', { path, project: state.project });
    state.projectPath = path;
    toast('Project saved', 'ok');
  } catch (e) { toast(`Save failed: ${e.message}`, 'error'); }
}

async function openProject() {
  try {
    const path = await openFileDialog({ multiple: false, filters: [{ name: 'PathLess project', extensions: ['json'] }] });
    if (!path) return;
    const p = await call('load_project', { path });
    resetProject(migrate(p));
    state.projectPath = path;
    const pts = [...state.project.area, ...state.project.sites.map((s) => [s.lon, s.lat])];
    mapView.fitTo(pts);
    toast('Project opened — fetch data to analyze', 'ok');
  } catch (e) { toast(`Open failed: ${e.message}`, 'error'); }
}

function resetProject(p) {
  state.project = p;
  state.projectPath = null;
  state.selectedSite = state.selectedLink = null;
  state.linkReports = {}; state.linkStatus = {};
  state.plan = null;
  state.coverageResult = null;
  mapView.clearCoverage();
  renderLegend();
  closeDrawer();
  $('#project-name').value = p.name;
  changed({ panel: true });
}

// ----------------------------------------------------------------- init ----
function initChrome() {
  const bm = $('#basemap');
  bm.innerHTML = Object.entries(BASEMAPS).map(([k, b]) => `<option value="${k}">${b.name}</option>`).join('');
  bm.addEventListener('change', () => mapView.setBasemap(bm.value));
  $('#ov-hillshade').addEventListener('change', (e) => mapView.setOverlay('hillshade', e.target.checked));
  $('#ov-nlcd').addEventListener('change', (e) => mapView.setOverlay('nlcd', e.target.checked));
  $('#ov-coverage').addEventListener('change', (e) => mapView.setOverlay('coverage', e.target.checked));
  $('#ov-opacity').addEventListener('input', (e) => mapView.setCoverageOpacity(e.target.value / 100));
  $$('#toolbar [data-mode]').forEach((b) => b.addEventListener('click', () => setMode(b.dataset.mode)));
  $$('#tabs button').forEach((b) => b.addEventListener('click', () => switchTab(b.dataset.tab)));
  $('#btn-fetch').addEventListener('click', fetchData);
  $('#btn-save').addEventListener('click', saveProject);
  $('#btn-open').addEventListener('click', openProject);
  $('#btn-new').addEventListener('click', () => { if (confirm('Start a new project? Unsaved changes will be lost.')) resetProject(newProject()); });
  $('#project-name').value = state.project.name;
  $('#project-name').addEventListener('change', (e) => { state.project.name = e.target.value; autosave(); });
  window.addEventListener('keydown', (e) => {
    if (e.target.matches('input, select, textarea')) return;
    if (e.key === 'Escape') setMode('none');
    if (state.mode === 'drawArea' && e.key === 'Enter') mapView.finishDraft();
    if (state.mode === 'drawArea' && e.key === 'Backspace') { e.preventDefault(); mapView.undoDraftPoint(); }
  });
}

async function init() {
  initChrome();
  await loadCatalog();
  if (hasBackend()) {
    try { state.dataStatus = await call('data_status'); } catch { state.dataStatus = null; }
  } else {
    toast('Running without the PathLess backend — UI preview only.', 'error');
  }
  renderPanel();
  refreshGroundElevations();
  await mapView.ready;
  renderMap();
  const pts = [...state.project.area, ...state.project.sites.map((s) => [s.lon, s.lat])];
  if (pts.length) mapView.fitTo(pts);
}

init();
