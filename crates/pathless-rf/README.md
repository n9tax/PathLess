# pathless-rf

Pure-Rust RF math for PathLess: ITU-R propagation models, link-budget helpers and antenna
patterns. There is no `unsafe`, no global mutable state, every function is pure and every
public type is `Send + Sync` and serde `Serialize`/`Deserialize`. Longley-Rice / ITM is
**not** part of this crate (see `pathless-itm`).

Units: frequency in MHz unless a name ends in `_ghz`; distance in metres unless it ends in
`_km`; heights in metres; angles in degrees; levels in dB / dBm / dBi.

## Modules

| Module | Contents |
|---|---|
| `basic` (alias `units`) | wavelength, FSPL (`32.4478 + 20 log f + 20 log d`), dBm/W/dBW, dBd↔dBi, Fresnel radius, earth bulge (`d1·d2/(2·k·6371 km)`), EIRP/ERP, thermal noise floor (`−174 + 10 log BW + NF`) |
| `diffraction` | P.526 knife edge J(ν), ν parameter, Deygout (3 edges, classic or P.452-14 empirical), Bullington (P.526-16 §4.5.1), delta-Bullington (P.526-16 §4.5.2) with smooth-earth §3.2, LOS / Fresnel clearance profile for plotting |
| `itu::p838` | rain specific attenuation k, α (H, V, any tilt and elevation), γR |
| `itu::p837` | P.837-1 rain zones A–Q table, `RainRate { Zone, Custom }` |
| `itu::p530` | multipath (P.530-17 and P.530-18/19 models), all-fade-depth method, worst-month→year (ΔG and P.841), rain attenuation A0.01 / Ap and its inverse |
| `itu::p676` | gaseous specific attenuation γo, γw (Annex 1 line-by-line) |
| `itu::p841` | annual ↔ worst-month conversion |
| `itu::availability` | `microwave_availability()` → `MicrowaveAvailability` (multipath + rain outage, seconds/minutes per year) |
| `antenna` | `AntennaPattern`, 3-D gain (summing / Gil cross-weighted), mechanical-tilt rotation, MSI reader/writer, generators, built-in catalog |
| `budget` | `link_budget()`, dBµV/m ↔ dBm, field strength from EIRP/ERP and path loss |

## ITU-R editions implemented

| Recommendation | Edition | Notes |
|---|---|---|
| P.525 | – | free-space loss formula (constant 32.4478 for km, MHz) |
| P.526 | **-16 (11/2025)** | eq. (26) ν, eq. (31) J(ν), §3.1.1/§3.2 smooth earth, §4.5.1 Bullington, §4.5.2 delta-Bullington |
| P.452 | -14 (10/2009) | Deygout with slope corrections + empirical combination (§4.2.1, eqs. 14–23), offered as a Deygout variant only |
| P.530 | **-17 (12/2017)**, **-18 (09/2021)**, **-19 (09/2025)** | multipath: -17 eqs. (4)–(8) (K from dN1, sa) *and* -18/-19 eqs. (5)–(11) (K, dN75, hc; maps supplied by caller); §2.3.2 all fade depths; §2.3.4 ΔG (-17/-18) and P.841 (-19) conversion; rain §2.4.1 per -19 (eq. 32 input limits) with a -17 variant |
| P.676 | **-13 (08/2022)** | Annex 1 line-by-line (Tables 1/2). The closed-form Annex 2 approximations of P.676-11 and earlier no longer exist in the current edition. |
| P.837 | -1 (1994) | Table 1 rain zones; gridded P.837-7 is out of scope |
| P.838 | **-3 (03/2005)** | current edition |
| P.841 | **-7 (08/2022)** | Q(p; Q1, β), global Q1 = 2.85, β = 0.13 |
| F.1336 | **-5 (01/2019)** | omni (rec. 2.1/2.2), sector (rec. 3.1.1/3.1.2), electrical tilt eq. (1e), mechanical tilt eqs. (3b)/(3c) |
| F.699 | **-9 (02/2026)** | dish peak envelope, 100 MHz–174.8 GHz, D/λ > 100 and ≤ 100, 70–86 GHz variant |
| F.1245 | **-3 (01/2019)** | dish average pattern 1–86 GHz |

Coefficient tables were transcribed from the Recommendation PDFs and cross-checked with
the MIT-licensed [ITU-Rpy](https://github.com/inigodelportillo/ITU-Rpy) data files.

### Validation

* **P.838-3** – reproduces Table 5 of the Recommendation (14 frequencies 1–1000 GHz,
  kH, αH, kV, αV) and the ITU-R SG3 validation set `ITURP838-3_rain_specific_attenuation.csv`
  to < 1e-6 relative.
* **P.676-13** – reproduces γo and γw of the ITU-R SG3 validation set
  `ITURP676-13_gamma.csv` (1013.25 hPa, 288.15 K, 7.5 g/m³, 20 frequencies 1–350 GHz incl.
  the 22, 60, 118, 183 and 325 GHz lines) to < 1e-6 relative.
* **P.841-7** – matches eq. (7) `p = 0.30·pw^1.15` and its exact closed form.
* **P.530** – unit tests check each equation against hand-evaluated formulas, continuity of
  the deep/shallow fading transition, monotonicity, and the exact inverse of eq. (34).
* **P.526** – J(0) = 6.03 dB, J(1) = 13.93 dB, single-edge Deygout == Bullington == J(ν).
* **F.1336/F.699/F.1245** – 3 dB points, side-lobe plateaus, region continuity, gain formula.

## Diffraction profiles

`distances_m` and `heights_m` are equally long; the first sample is the transmitter site,
the last the receiver. Heights are **raw** terrain (+ clutter) AMSL; earth curvature is
applied internally from `PathParams::k_factor`. Antenna heights in `PathParams` are AMSL.
`los_clearance()` returns, per sample, the straight-ray height, bulged terrain, F1 radius,
clearance and clearance/F1, plus the minimum ratio and its location.

For automatic processing prefer `delta_bullington_loss()` (the current ITU-R method).
Classic Deygout over-estimates on finely sampled rounded hills (neighbouring samples of
the same hill become secondary edges).

## Antenna patterns

`AntennaPattern.horizontal[i]`: attenuation (dB re peak) at azimuth *i*° clockwise from
boresight. `vertical[i]`: attenuation at *i*° **below** the horizon (MSI/Planet: 0 horizon,
90 nadir, 180 back horizon, 270 zenith). API arguments use elevation **above** horizon.

3-D gain (`gain_dbi`, `gain_dbi_with`): `InterpolationMethod::Weighted` (default; Gil et al.
2001 cross-weighted interpolation, exact on both cuts, front/back blended) or
`InterpolationMethod::Summing` (`H(φ) + V(θ)`, back half re-referenced, clamped to the
deepest cut value). `antenna_relative_direction()` / `gain_toward()` apply mechanical
downtilt by a true rotation (F.1336 eqs. 3b/3c).

Note: the MSI format has 1° resolution; for high-gain dishes (beamwidth of 1–3°) the
linear interpolation between 1° samples is coarse off boresight.

### MSI / Planet format notes

* Header keywords (case-insensitive): `NAME`, `MAKE`, `FREQUENCY` (MHz; `GHz` suffix
  accepted), `H_WIDTH`, `V_WIDTH`, `FRONT_TO_BACK`, `GAIN`, `TILT`, `ELECTRICAL_TILT`,
  `MECHANICAL_TILT`, `POLARIZATION`, `COMMENT`, then `HORIZONTAL n` and `VERTICAL n`
  followed by `angle value` lines. Unknown keywords are ignored.
* `GAIN` without a unit is **dBd** (Planet convention; override with
  `MsiParseOptions::default_gain_unit`). `dBi` / `dBd` suffixes are honoured. The writer
  always writes an explicit unit (dBi by default).
* Leniency: tabs/spaces/`;`/`=` separators, decimal commas, `a,b` pairs, extra columns,
  fractional angles, fewer or more than 360 points (resampled to 1° with circular linear
  interpolation), duplicate 360° sample, UTF-8 BOM, a bare name on the first line,
  relative-gain files (all values ≤ 0) are negated.
* Missing `H_WIDTH`/`V_WIDTH`/`FRONT_TO_BACK` are measured from the cuts.
* Mechanical tilt in a file is ignored (installation property); electrical tilt is stored
  informationally (it is already contained in the vertical cut).

## Built-in catalog

`builtin_catalog()` parses `data/antenna_catalog.json` (embedded with `include_str!`).
Entries are generic, non-vendor profiles whose patterns are generated on demand at the
analysis frequency (`AntennaSpec::pattern_at(f_mhz)`): dipoles/whips, F.1336 omnis
(3–12 dBi, VHF→5 GHz), F.1336 sectors (65/90/120°, 700 MHz→6 GHz) and panels, empirical
yagis (3/6/9/13 elements, VHF/UHF/900), F.699 dishes 0.3–3.0 m (1–86 GHz) plus F.1245
average variants, WISP dishes and grids. `load_catalog_from_dir()` merges user `.msi/.pln/
.ant/.txt` patterns (id `user:<stem>`) and `.json` specs (single object or array; same id
replaces a built-in).
