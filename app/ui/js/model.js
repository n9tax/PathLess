// Project model. Field names mirror the Rust `pathless_engine::project` structs
// (serde camelCase), so the whole object is passed to the backend as-is.

export const SITE_COLORS = ['#f5b13d', '#3fa7ff', '#3ccf7a', '#ff6b9a', '#b38cff', '#4fd1c5', '#ff8a4c', '#e6e15a'];

let counter = 0;
export function uid(prefix) {
  counter += 1;
  return `${prefix}-${Date.now().toString(36)}-${counter}`;
}

export const NLCD_CLASSES = [
  [11, 'Open water', '#466b9f'],
  [12, 'Perennial ice/snow', '#d1def8'],
  [21, 'Developed, open space', '#dec5c5'],
  [22, 'Developed, low intensity', '#d99282'],
  [23, 'Developed, medium intensity', '#eb0000'],
  [24, 'Developed, high intensity', '#ab0000'],
  [31, 'Barren land', '#b3ac9f'],
  [41, 'Deciduous forest', '#68ab5f'],
  [42, 'Evergreen forest', '#1c5f2c'],
  [43, 'Mixed forest', '#b5c58f'],
  [51, 'Dwarf scrub (AK)', '#af963c'],
  [52, 'Shrub/scrub', '#ccb879'],
  [71, 'Grassland/herbaceous', '#dfdfc2'],
  [72, 'Sedge/herbaceous (AK)', '#d1d182'],
  [73, 'Lichens (AK)', '#a3cc51'],
  [74, 'Moss (AK)', '#82ba9e'],
  [81, 'Pasture/hay', '#dcd939'],
  [82, 'Cultivated crops', '#ab6c28'],
  [90, 'Woody wetlands', '#b8d9eb'],
  [95, 'Emergent herbaceous wetlands', '#6c9fb8'],
];

// Representative obstruction heights (m) added on top of bare-earth terrain.
export const DEFAULT_CLUTTER_HEIGHTS = {
  11: 0, 12: 0, 21: 2, 22: 6, 23: 10, 24: 18, 31: 0, 41: 15, 42: 18, 43: 15,
  51: 1, 52: 2, 71: 0.5, 72: 0.5, 73: 0, 74: 0, 81: 0.5, 82: 1, 90: 12, 95: 1,
};

export function newProject() {
  return {
    version: 1,
    name: 'Untitled project',
    area: [],
    data: { demResolution: 'arcSecThird', useClutter: true },
    env: {
      climate: 'continentalTemperate',
      n0: 301,
      ground: 'average',
      epsilon: 15,
      sigma: 0.005,
      kFactor: 4 / 3,
      timePct: 50,
      locationPct: 50,
      situationPct: 90,
      variability: 'accidental',
      model: 'auto',
      clutterHeights: { ...DEFAULT_CLUTTER_HEIGHTS },
      terminalClutterLoss: true,
    },
    sites: [],
    links: [],
    coverage: {
      siteId: null,
      frequencyMhz: 450,
      polarization: 'vertical',
      rxHeightM: 1.5,
      rxGainDbi: 0,
      rxLineLossDb: 0,
      resolutionM: 30,
      maxRadiusKm: 25,
      output: 'rxDbm',
      legend: defaultLegend('rxDbm'),
    },
  };
}

export function newSite(lat, lon, index) {
  return {
    id: uid('site'),
    name: `Site ${index + 1}`,
    lat, lon,
    color: SITE_COLORS[index % SITE_COLORS.length],
    antennaHeightM: 30,
    antennaId: null,
    azimuthDeg: 0,
    downtiltDeg: 0,
    txPowerDbm: 40,
    lineLossDb: 2,
    otherLossDb: 0,
    rxSensitivityDbm: -100,
  };
}

export function newLink(siteA, siteB, index) {
  return {
    id: uid('link'),
    name: `Link ${index + 1}`,
    siteA, siteB,
    frequencyMhz: 5800,
    polarization: 'vertical',
    rainZone: 'K',
    customR001: null,
    dn1: -200,
    targetAvailabilityPct: 99.99,
    alignAntennas: true,
  };
}

export function defaultLegend(output) {
  if (output === 'fieldDbuvm') {
    return [
      { value: 70, color: '#1a9850' }, { value: 60, color: '#66bd63' }, { value: 50, color: '#a6d96a' },
      { value: 40, color: '#fee08b' }, { value: 30, color: '#fdae61' }, { value: 20, color: '#f46d43' },
    ];
  }
  if (output === 'pathLossDb') {
    return [
      { value: 100, color: '#1a9850' }, { value: 120, color: '#66bd63' }, { value: 130, color: '#a6d96a' },
      { value: 140, color: '#fee08b' }, { value: 150, color: '#fdae61' }, { value: 160, color: '#f46d43' },
    ];
  }
  return [
    { value: -60, color: '#1a9850' }, { value: -70, color: '#66bd63' }, { value: -80, color: '#a6d96a' },
    { value: -90, color: '#fee08b' }, { value: -100, color: '#fdae61' }, { value: -110, color: '#f46d43' },
  ];
}

export const CLIMATES = [
  ['equatorial', 'Equatorial'],
  ['continentalSubtropical', 'Continental subtropical'],
  ['maritimeSubtropical', 'Maritime subtropical'],
  ['desert', 'Desert'],
  ['continentalTemperate', 'Continental temperate'],
  ['maritimeTemperateOverLand', 'Maritime temperate (land)'],
  ['maritimeTemperateOverSea', 'Maritime temperate (sea)'],
];

export const GROUNDS = {
  average: ['Average ground', 15, 0.005],
  poor: ['Poor ground', 4, 0.001],
  good: ['Good ground', 25, 0.02],
  freshWater: ['Fresh water', 81, 0.01],
  sea: ['Sea water', 81, 5.0],
  custom: ['Custom', null, null],
};

export const RAIN_ZONES = 'ABCDEFGHJKLMNPQ'.split('');

export const DEM_RESOLUTIONS = [
  ['arcSec1', '1 arc-second (~30 m) — fast, nationwide'],
  ['arcSecThird', '1/3 arc-second (~10 m) — recommended'],
  ['arcSecNinth', '1/9 arc-second (~3 m) — lidar areas'],
  ['meter1', '1 m — lidar areas, small areas only'],
];
