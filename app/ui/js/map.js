// MapLibre map: basemaps, overlays, study-area drawing, site markers, links
// and the coverage image overlay.

const USGS = 'https://basemap.nationalmap.gov/arcgis/rest/services';

export const BASEMAPS = {
  usgsTopo: { name: 'USGS Topo', tiles: [`${USGS}/USGSTopo/MapServer/tile/{z}/{y}/{x}`], maxzoom: 16, attribution: 'USGS The National Map' },
  usgsImageryTopo: { name: 'USGS Imagery + Topo', tiles: [`${USGS}/USGSImageryTopo/MapServer/tile/{z}/{y}/{x}`], maxzoom: 16, attribution: 'USGS The National Map' },
  usgsImagery: { name: 'USGS Imagery', tiles: [`${USGS}/USGSImageryOnly/MapServer/tile/{z}/{y}/{x}`], maxzoom: 16, attribution: 'USGS The National Map' },
  usgsRelief: { name: 'USGS Shaded Relief', tiles: [`${USGS}/USGSShadedReliefOnly/MapServer/tile/{z}/{y}/{x}`], maxzoom: 16, attribution: 'USGS The National Map' },
  osm: { name: 'OpenStreetMap', tiles: ['https://tile.openstreetmap.org/{z}/{x}/{y}.png'], maxzoom: 19, attribution: '© OpenStreetMap contributors' },
};

const NLCD_WMS = 'https://www.mrlc.gov/geoserver/mrlc_display/NLCD_2021_Land_Cover_L48/wms?service=WMS&version=1.1.1&request=GetMap&layers=NLCD_2021_Land_Cover_L48&styles=&format=image/png&transparent=true&srs=EPSG:3857&width=256&height=256&bbox={bbox-epsg-3857}';

export class MapView {
  constructor(container, handlers) {
    this.h = handlers; // { onMapClick, onAreaChanged, onSiteMoved, onSiteClick, onLinkClick, onMouseMove }
    this.siteMarkers = new Map();
    this.vertexMarkers = [];
    this.draft = null; // in-progress polygon [[lon,lat],...]
    this.mode = 'none';

    const style = {
      version: 8,
      sources: {},
      layers: [],
    };
    for (const [id, b] of Object.entries(BASEMAPS)) {
      style.sources[`bm-${id}`] = { type: 'raster', tiles: b.tiles, tileSize: 256, maxzoom: b.maxzoom, attribution: b.attribution };
      style.layers.push({ id: `bm-${id}`, type: 'raster', source: `bm-${id}`, layout: { visibility: id === 'usgsTopo' ? 'visible' : 'none' } });
    }
    style.sources.hillshade = { type: 'raster', tiles: [`${USGS}/USGSShadedReliefOnly/MapServer/tile/{z}/{y}/{x}`], tileSize: 256, maxzoom: 16 };
    style.layers.push({ id: 'hillshade', type: 'raster', source: 'hillshade', paint: { 'raster-opacity': 0.35 }, layout: { visibility: 'none' } });
    style.sources.nlcd = { type: 'raster', tiles: [NLCD_WMS], tileSize: 256, attribution: 'NLCD / MRLC' };
    style.layers.push({ id: 'nlcd', type: 'raster', source: 'nlcd', paint: { 'raster-opacity': 0.6 }, layout: { visibility: 'none' } });

    this.map = new maplibregl.Map({
      container,
      style,
      center: [-98.5, 39.5],
      zoom: 4,
      doubleClickZoom: false,
      attributionControl: { compact: true },
    });
    this.map.addControl(new maplibregl.NavigationControl({ visualizePitch: false }), 'top-right');
    this.map.addControl(new maplibregl.ScaleControl({ unit: 'metric' }), 'bottom-left');

    this.ready = new Promise((resolve) => this.map.on('load', () => { this._initLayers(); resolve(); }));
    this.map.on('click', (e) => this._onClick(e));
    this.map.on('dblclick', (e) => { if (this.mode === 'drawArea') { e.preventDefault(); this.finishDraft(); } });
    this.map.on('mousemove', (e) => {
      if (this.draft) this._renderDraft([e.lngLat.lng, e.lngLat.lat]);
      this.h.onMouseMove?.(e.lngLat);
    });
  }

  _initLayers() {
    const m = this.map;
    const empty = { type: 'FeatureCollection', features: [] };
    m.addSource('area', { type: 'geojson', data: empty });
    m.addLayer({ id: 'area-fill', type: 'fill', source: 'area', paint: { 'fill-color': '#f5a524', 'fill-opacity': 0.08 } });
    m.addLayer({ id: 'area-line', type: 'line', source: 'area', paint: { 'line-color': '#f5a524', 'line-width': 2, 'line-dasharray': [3, 2] } });
    m.addSource('draft', { type: 'geojson', data: empty });
    m.addLayer({ id: 'draft-fill', type: 'fill', source: 'draft', filter: ['==', '$type', 'Polygon'], paint: { 'fill-color': '#3fa7ff', 'fill-opacity': 0.12 } });
    m.addLayer({ id: 'draft-line', type: 'line', source: 'draft', paint: { 'line-color': '#3fa7ff', 'line-width': 2 } });
    m.addLayer({ id: 'draft-pts', type: 'circle', source: 'draft', filter: ['==', '$type', 'Point'], paint: { 'circle-radius': 4, 'circle-color': '#fff', 'circle-stroke-color': '#3fa7ff', 'circle-stroke-width': 2 } });
    m.addSource('links', { type: 'geojson', data: empty });
    m.addLayer({ id: 'links-casing', type: 'line', source: 'links', paint: { 'line-color': '#000', 'line-width': 6, 'line-opacity': 0.45 } });
    m.addLayer({
      id: 'links', type: 'line', source: 'links',
      paint: { 'line-color': ['get', 'color'], 'line-width': ['case', ['get', 'selected'], 4.5, 3] },
    });
    m.on('click', 'links', (e) => { e.originalEvent._handled = true; this.h.onLinkClick?.(e.features[0].properties.id); });
    m.on('mouseenter', 'links', () => { if (this.mode === 'none') m.getCanvas().style.cursor = 'pointer'; });
    m.on('mouseleave', 'links', () => { if (this.mode === 'none') m.getCanvas().style.cursor = ''; });
  }

  setBasemap(id) {
    for (const k of Object.keys(BASEMAPS)) this.map.setLayoutProperty(`bm-${k}`, 'visibility', k === id ? 'visible' : 'none');
  }

  setOverlay(id, on) {
    if (this.map.getLayer(id)) this.map.setLayoutProperty(id, 'visibility', on ? 'visible' : 'none');
  }

  setMode(mode) {
    this.mode = mode;
    this.map.getCanvas().style.cursor = mode === 'none' ? '' : 'crosshair';
    if (mode === 'drawArea') {
      this.draft = [];
      this._renderDraft();
    } else if (this.draft) {
      this.draft = null;
      this._renderDraft();
    }
  }

  _onClick(e) {
    if (e.originalEvent._handled) return;
    if (this.mode === 'drawArea') {
      // Clicking near the first vertex closes the polygon.
      if (this.draft.length >= 3) {
        const first = this.map.project(this.draft[0]);
        if (Math.hypot(first.x - e.point.x, first.y - e.point.y) < 10) { this.finishDraft(); return; }
      }
      this.draft.push([e.lngLat.lng, e.lngLat.lat]);
      this._renderDraft();
      return;
    }
    this.h.onMapClick?.(e.lngLat);
  }

  undoDraftPoint() {
    if (this.draft && this.draft.length) { this.draft.pop(); this._renderDraft(); }
  }

  finishDraft() {
    if (!this.draft) return;
    // A dblclick also fires two clicks; drop the duplicated trailing vertex.
    const pts = this.draft.filter((p, i, a) => i === 0 || Math.abs(p[0] - a[i - 1][0]) + Math.abs(p[1] - a[i - 1][1]) > 1e-9);
    this.draft = null;
    this._renderDraft();
    if (pts.length >= 3) this.h.onAreaChanged?.(pts, true);
  }

  _renderDraft(cursor) {
    const src = this.map.getSource('draft');
    if (!src) return;
    const feats = [];
    if (this.draft && this.draft.length) {
      const ring = cursor ? [...this.draft, cursor] : [...this.draft];
      for (const p of this.draft) feats.push({ type: 'Feature', geometry: { type: 'Point', coordinates: p } });
      if (ring.length >= 3) feats.push({ type: 'Feature', geometry: { type: 'Polygon', coordinates: [[...ring, ring[0]]] } });
      else if (ring.length === 2) feats.push({ type: 'Feature', geometry: { type: 'LineString', coordinates: ring } });
    }
    src.setData({ type: 'FeatureCollection', features: feats });
  }

  renderArea(area) {
    const src = this.map.getSource('area');
    if (!src) return;
    src.setData(area.length >= 3
      ? { type: 'Feature', geometry: { type: 'Polygon', coordinates: [[...area, area[0]]] } }
      : { type: 'FeatureCollection', features: [] });
    for (const mk of this.vertexMarkers) mk.remove();
    this.vertexMarkers = area.map((p, i) => {
      const el = document.createElement('div');
      el.className = 'vertex-marker';
      el.title = 'Drag to move · right-click to delete';
      const mk = new maplibregl.Marker({ element: el, draggable: true }).setLngLat(p).addTo(this.map);
      mk.on('drag', () => {
        const ll = mk.getLngLat();
        area[i] = [ll.lng, ll.lat];
        src.setData({ type: 'Feature', geometry: { type: 'Polygon', coordinates: [[...area, area[0]]] } });
      });
      mk.on('dragend', () => this.h.onAreaChanged?.(area, false));
      el.addEventListener('contextmenu', (ev) => {
        ev.preventDefault();
        if (area.length > 3) { area.splice(i, 1); this.h.onAreaChanged?.(area, false); }
      });
      return mk;
    });
  }

  renderSites(sites, selectedId) {
    const seen = new Set();
    sites.forEach((s, idx) => {
      seen.add(s.id);
      let mk = this.siteMarkers.get(s.id);
      if (!mk) {
        const el = document.createElement('div');
        el.className = 'site-marker';
        el.innerHTML = '<span class="num"></span><span class="site-label"></span>';
        mk = new maplibregl.Marker({ element: el, draggable: true }).setLngLat([s.lon, s.lat]).addTo(this.map);
        mk.on('dragend', () => { const ll = mk.getLngLat(); this.h.onSiteMoved?.(s.id, ll.lat, ll.lng); });
        el.addEventListener('click', (ev) => { ev.stopPropagation(); this.h.onSiteClick?.(s.id); });
        this.siteMarkers.set(s.id, mk);
      }
      mk.setLngLat([s.lon, s.lat]);
      const el = mk.getElement();
      el.style.background = s.color;
      el.style.outline = s.id === selectedId ? '3px solid rgba(255,255,255,.55)' : 'none';
      el.querySelector('.num').textContent = idx + 1;
      el.querySelector('.site-label').textContent = s.name;
    });
    for (const [id, mk] of this.siteMarkers) {
      if (!seen.has(id)) { mk.remove(); this.siteMarkers.delete(id); }
    }
  }

  renderLinks(links, sites, selectedId, statusById = {}) {
    const byId = Object.fromEntries(sites.map((s) => [s.id, s]));
    const feats = [];
    for (const l of links) {
      const a = byId[l.siteA], b = byId[l.siteB];
      if (!a || !b) continue;
      const st = statusById[l.id];
      const color = st === 'good' ? '#3ccf7a' : st === 'warn' ? '#f5c542' : st === 'bad' ? '#ff5d5d' : '#e8eef5';
      feats.push({
        type: 'Feature',
        properties: { id: l.id, color, selected: l.id === selectedId },
        geometry: { type: 'LineString', coordinates: [[a.lon, a.lat], [b.lon, b.lat]] },
      });
    }
    this.map.getSource('links')?.setData({ type: 'FeatureCollection', features: feats });
  }

  showCoverage(imageUrl, bounds, opacity) {
    // bounds: [west, south, east, north]
    const coords = [[bounds[0], bounds[3]], [bounds[2], bounds[3]], [bounds[2], bounds[1]], [bounds[0], bounds[1]]];
    const src = this.map.getSource('coverage');
    if (src) {
      src.updateImage({ url: imageUrl, coordinates: coords });
    } else {
      this.map.addSource('coverage', { type: 'image', url: imageUrl, coordinates: coords });
      this.map.addLayer({ id: 'coverage', type: 'raster', source: 'coverage', paint: { 'raster-opacity': opacity, 'raster-resampling': 'nearest' } }, 'area-fill');
    }
  }

  clearCoverage() {
    if (this.map.getLayer('coverage')) this.map.removeLayer('coverage');
    if (this.map.getSource('coverage')) this.map.removeSource('coverage');
  }

  setCoverageOpacity(o) {
    if (this.map.getLayer('coverage')) this.map.setPaintProperty('coverage', 'raster-opacity', o);
  }

  fitTo(points) {
    if (!points.length) return;
    const b = new maplibregl.LngLatBounds(points[0], points[0]);
    for (const p of points) b.extend(p);
    this.map.fitBounds(b, { padding: 80, maxZoom: 14, duration: 600 });
  }
}
