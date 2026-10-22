// Canvas charts: link path profile and antenna polar patterns.

function setupCanvas(canvas) {
  const dpr = window.devicePixelRatio || 1;
  const r = canvas.getBoundingClientRect();
  canvas.width = Math.max(1, Math.round(r.width * dpr));
  canvas.height = Math.max(1, Math.round(r.height * dpr));
  const ctx = canvas.getContext('2d');
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { ctx, w: r.width, h: r.height };
}

function niceStep(range, target) {
  const raw = range / target;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const n = raw / mag;
  return (n < 1.5 ? 1 : n < 3 ? 2 : n < 7 ? 5 : 10) * mag;
}

/**
 * Path profile chart. `p` is LinkReport.profile from the backend:
 * distanceM, terrainM, clutterM, bulgeM, losM, fresnelM (all same length).
 * Terrain is drawn on a curved earth (terrain + bulge) so the line of sight
 * stays straight.
 */
export class ProfileChart {
  constructor(canvas, tip) {
    this.canvas = canvas;
    this.tip = tip;
    this.data = null;
    canvas.addEventListener('mousemove', (e) => this._hover(e));
    canvas.addEventListener('mouseleave', () => { this.tip.classList.add('hidden'); this.hoverIdx = null; this.draw(); });
    new ResizeObserver(() => this.draw()).observe(canvas);
  }

  setData(report) {
    this.report = report;
    this.data = report?.profile || null;
    this.draw();
  }

  draw() {
    const { ctx, w, h } = setupCanvas(this.canvas);
    ctx.clearRect(0, 0, w, h);
    const p = this.data;
    if (!p || !p.distanceM?.length) {
      ctx.fillStyle = '#8ea2b8';
      ctx.font = '13px sans-serif';
      ctx.fillText('Analyze a link to see its path profile.', 20, 30);
      return;
    }
    const n = p.distanceM.length;
    const ground = p.terrainM.map((t, i) => t + p.bulgeM[i]);
    const top = ground.map((g, i) => g + (p.clutterM?.[i] || 0));
    let ymin = Math.min(...ground), ymax = -Infinity;
    for (let i = 0; i < n; i++) ymax = Math.max(ymax, top[i], p.losM[i] + p.fresnelM[i]);
    const pad = (ymax - ymin) * 0.08 + 5;
    ymin -= pad; ymax += pad;
    const L = 58, R = 16, Tm = 14, B = 30;
    const xmax = p.distanceM[n - 1];
    const X = (d) => L + (d / xmax) * (w - L - R);
    const Y = (v) => Tm + (1 - (v - ymin) / (ymax - ymin)) * (h - Tm - B);
    this._geom = { X, Y, L, R, w, h, xmax };

    // grid
    ctx.strokeStyle = '#223044'; ctx.fillStyle = '#8ea2b8'; ctx.font = '11px sans-serif'; ctx.lineWidth = 1;
    const ys = niceStep(ymax - ymin, 6);
    for (let v = Math.ceil(ymin / ys) * ys; v <= ymax; v += ys) {
      ctx.beginPath(); ctx.moveTo(L, Y(v)); ctx.lineTo(w - R, Y(v)); ctx.stroke();
      ctx.fillText(`${Math.round(v)} m`, 4, Y(v) + 4);
    }
    const xs = niceStep(xmax / 1000, 8);
    for (let d = 0; d <= xmax / 1000 + 1e-9; d += xs) {
      ctx.beginPath(); ctx.moveTo(X(d * 1000), Tm); ctx.lineTo(X(d * 1000), h - B); ctx.stroke();
      ctx.fillText(`${+d.toFixed(2)} km`, X(d * 1000) - 14, h - B + 16);
    }

    // clutter
    ctx.beginPath();
    for (let i = 0; i < n; i++) { const x = X(p.distanceM[i]), y = Y(top[i]); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); }
    for (let i = n - 1; i >= 0; i--) ctx.lineTo(X(p.distanceM[i]), Y(ground[i]));
    ctx.closePath(); ctx.fillStyle = 'rgba(76, 160, 90, .55)'; ctx.fill();

    // terrain
    ctx.beginPath();
    ctx.moveTo(X(0), h - B);
    for (let i = 0; i < n; i++) ctx.lineTo(X(p.distanceM[i]), Y(ground[i]));
    ctx.lineTo(X(xmax), h - B); ctx.closePath();
    const grad = ctx.createLinearGradient(0, Tm, 0, h - B);
    grad.addColorStop(0, '#9b7b52'); grad.addColorStop(1, '#4a3a28');
    ctx.fillStyle = grad; ctx.fill();
    ctx.strokeStyle = '#c9a675'; ctx.lineWidth = 1.2;
    ctx.beginPath();
    for (let i = 0; i < n; i++) { const x = X(p.distanceM[i]), y = Y(ground[i]); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); }
    ctx.stroke();

    // Fresnel zone
    ctx.beginPath();
    for (let i = 0; i < n; i++) { const x = X(p.distanceM[i]), y = Y(p.losM[i] + p.fresnelM[i]); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); }
    for (let i = n - 1; i >= 0; i--) ctx.lineTo(X(p.distanceM[i]), Y(p.losM[i] - p.fresnelM[i]));
    ctx.closePath(); ctx.fillStyle = 'rgba(63, 167, 255, .13)'; ctx.fill();
    ctx.strokeStyle = 'rgba(63, 167, 255, .7)'; ctx.lineWidth = 1; ctx.stroke();
    // 60% F1 lower bound
    ctx.setLineDash([4, 4]); ctx.strokeStyle = 'rgba(63, 167, 255, .9)';
    ctx.beginPath();
    for (let i = 0; i < n; i++) { const x = X(p.distanceM[i]), y = Y(p.losM[i] - 0.6 * p.fresnelM[i]); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); }
    ctx.stroke(); ctx.setLineDash([]);

    // line of sight
    ctx.strokeStyle = '#f5b13d'; ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(X(0), Y(p.losM[0])); ctx.lineTo(X(xmax), Y(p.losM[n - 1])); ctx.stroke();

    // antennas / masts
    for (const [i, label] of [[0, this.report.siteA?.name], [n - 1, this.report.siteB?.name]]) {
      const x = X(p.distanceM[i]);
      ctx.strokeStyle = '#e8eef5'; ctx.lineWidth = 2;
      ctx.beginPath(); ctx.moveTo(x, Y(ground[i])); ctx.lineTo(x, Y(p.losM[i])); ctx.stroke();
      ctx.fillStyle = '#fff'; ctx.beginPath(); ctx.arc(x, Y(p.losM[i]), 4, 0, Math.PI * 2); ctx.fill();
      if (label) { ctx.font = '12px sans-serif'; ctx.fillText(label, i ? x - ctx.measureText(label).width - 8 : x + 8, Y(p.losM[i]) - 6); }
    }

    // worst clearance point
    const wi = this.report.clearance?.worstIndex;
    if (wi != null && wi >= 0 && wi < n) {
      const x = X(p.distanceM[wi]);
      ctx.strokeStyle = '#ff5d5d'; ctx.lineWidth = 1.5;
      ctx.beginPath(); ctx.moveTo(x, Y(top[wi])); ctx.lineTo(x, Y(p.losM[wi])); ctx.stroke();
      ctx.fillStyle = '#ff5d5d'; ctx.beginPath(); ctx.arc(x, Y(top[wi]), 3.5, 0, Math.PI * 2); ctx.fill();
    }

    if (this.hoverIdx != null) {
      const x = X(p.distanceM[this.hoverIdx]);
      ctx.strokeStyle = 'rgba(255,255,255,.4)'; ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(x, Tm); ctx.lineTo(x, h - B); ctx.stroke();
    }
  }

  _hover(e) {
    const p = this.data, g = this._geom;
    if (!p || !g) return;
    const r = this.canvas.getBoundingClientRect();
    const mx = e.clientX - r.left;
    const d = ((mx - g.L) / (g.w - g.L - g.R)) * g.xmax;
    if (d < 0 || d > g.xmax) { this.tip.classList.add('hidden'); return; }
    let lo = 0, hi = p.distanceM.length - 1;
    while (hi - lo > 1) { const mid = (lo + hi) >> 1; if (p.distanceM[mid] < d) lo = mid; else hi = mid; }
    const i = d - p.distanceM[lo] < p.distanceM[hi] - d ? lo : hi;
    this.hoverIdx = i;
    this.draw();
    const top = p.terrainM[i] + p.bulgeM[i] + (p.clutterM?.[i] || 0);
    const clr = p.losM[i] - top;
    const f1 = p.fresnelM[i];
    this.tip.innerHTML = `
      <b>${(p.distanceM[i] / 1000).toFixed(3)} km</b><br>
      Terrain ${p.terrainM[i].toFixed(1)} m${p.clutterM?.[i] ? ` + clutter ${p.clutterM[i].toFixed(1)} m` : ''}<br>
      Earth bulge ${p.bulgeM[i].toFixed(1)} m<br>
      LOS ${(p.losM[i]).toFixed(1)} m · clearance ${clr.toFixed(1)} m<br>
      F1 ${f1.toFixed(1)} m · ${f1 > 0 ? ((clr / f1) * 100).toFixed(0) : '—'}% F1`;
    this.tip.classList.remove('hidden');
    const tx = Math.min(mx + 14, r.width - this.tip.offsetWidth - 6);
    this.tip.style.left = `${tx}px`;
    this.tip.style.top = '10px';
  }
}

/**
 * Draws horizontal (left) and vertical (right) antenna pattern cuts.
 * `pat.horizontal`/`pat.vertical` are 360 attenuation values (dB below peak);
 * vertical index = degrees below horizon (MSI convention).
 */
export function drawPolar(canvas, pat) {
  const { ctx, w, h } = setupCanvas(canvas);
  ctx.clearRect(0, 0, w, h);
  if (!pat) return;
  const range = 40;
  const cuts = [['Horizontal', pat.horizontal, 0], ['Vertical', pat.vertical, 1]];
  for (const [title, cut, k] of cuts) {
    const cx = w / 4 + k * (w / 2), cy = h / 2 + 6, R = Math.min(w / 4, h / 2) - 16;
    ctx.strokeStyle = '#2a3a50'; ctx.lineWidth = 1; ctx.fillStyle = '#8ea2b8'; ctx.font = '10px sans-serif';
    for (const db of [0, 10, 20, 30]) {
      const r = R * (1 - db / range);
      ctx.beginPath(); ctx.arc(cx, cy, r, 0, Math.PI * 2); ctx.stroke();
    }
    for (let a = 0; a < 360; a += 30) {
      const t = (a - 90) * Math.PI / 180;
      ctx.beginPath(); ctx.moveTo(cx, cy); ctx.lineTo(cx + R * Math.cos(t), cy + R * Math.sin(t)); ctx.stroke();
    }
    ctx.fillText(title, cx - R, 12);
    if (!cut?.length) continue;
    ctx.beginPath();
    for (let i = 0; i <= cut.length; i++) {
      const idx = i % cut.length;
      const deg = idx * 360 / cut.length;
      const att = Math.min(range, Math.max(0, cut[idx]));
      const r = R * (1 - att / range);
      // Horizontal: 0° (boresight) up, clockwise. Vertical: boresight (horizon) to the right, below-horizon angles go down.
      const t = k === 0 ? (deg - 90) * Math.PI / 180 : deg * Math.PI / 180;
      const x = cx + r * Math.cos(t), y = cy + r * Math.sin(t);
      i ? ctx.lineTo(x, y) : ctx.moveTo(x, y);
    }
    ctx.closePath();
    ctx.fillStyle = 'rgba(245,177,61,.18)'; ctx.fill();
    ctx.strokeStyle = '#f5b13d'; ctx.lineWidth = 1.5; ctx.stroke();
  }
}
