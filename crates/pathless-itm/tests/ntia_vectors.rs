//! Runs every test vector shipped with the NTIA/ITS ITM reference
//! implementation (https://github.com/NTIA/itm, public domain):
//!
//! * `p2p.csv` + `pfls.csv` - point-to-point TLS mode (losses printed to 0.01 dB)
//! * `area.csv`             - area TLS mode (losses printed to 0.1 dB)
//! * `cmd_examples/`        - ITMDrvr inputs `i_*.txt` and outputs `o_*.txt`
//!   (losses, free-space loss, warnings and intermediate values, printed to
//!   0.1 dB / documented precision)
//!
//! Every value is checked to be consistent with the reference output at the
//! printed precision (|err| <= half a unit in the last printed digit), which
//! is far tighter than the 0.1 dB acceptance bound.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use pathless_itm::*;

fn data_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(rel)
}

fn read(rel: &str) -> String {
    fs::read_to_string(data_path(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// Tracks the worst error seen, for reporting.
#[derive(Default)]
struct Stats {
    n: usize,
    max_err: f64,
}

impl Stats {
    /// Assert `actual` rounds to `expected` (a decimal string) at the printed
    /// precision and is within 0.1 dB.
    fn check(&mut self, what: &str, actual: f64, expected: &str) {
        let expected = expected.trim();
        let e: f64 = expected.parse().unwrap_or_else(|_| panic!("{what}: bad number {expected:?}"));
        let decimals = expected.split_once('.').map(|(_, f)| f.len()).unwrap_or(0) as i32;
        let tol = 0.5 * 10f64.powi(-decimals) + 1e-9;
        let err = (actual - e).abs();
        assert!(err <= tol && err <= 0.1, "{what}: got {actual:.6}, expected {expected} (|err| = {err:.6}, tol {tol})");
        self.n += 1;
        if err > self.max_err {
            self.max_err = err;
        }
    }
}

fn parse_csv(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let header = lines.next().unwrap().split(',').map(|s| s.trim().to_string()).collect();
    let rows = lines.map(|l| l.split(',').map(|s| s.trim().to_string()).collect()).collect();
    (header, rows)
}

fn row_map<'a>(header: &'a [String], row: &'a [String]) -> HashMap<&'a str, &'a str> {
    header.iter().map(String::as_str).zip(row.iter().map(String::as_str)).collect()
}

fn f(m: &HashMap<&str, &str>, k: &str) -> f64 {
    m.get(k).unwrap_or_else(|| panic!("missing {k}")).parse().unwrap()
}

fn i(m: &HashMap<&str, &str>, k: &str) -> i32 {
    m.get(k).unwrap_or_else(|| panic!("missing {k}")).parse().unwrap()
}

#[test]
fn p2p_csv_vectors() {
    let (header, rows) = parse_csv(&read("p2p.csv"));
    let pfls: Vec<Vec<f64>> = read("pfls.csv")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.split(',').map(|v| v.trim().parse::<f64>().unwrap()).collect())
        .collect();
    assert_eq!(rows.len(), 5);
    assert_eq!(pfls.len(), rows.len());

    let mut stats = Stats::default();
    for (idx, (row, pfl)) in rows.iter().zip(&pfls).enumerate() {
        let m = row_map(&header, row);
        let params = P2pParams {
            h_tx_m: f(&m, "h_tx__meter"),
            h_rx_m: f(&m, "h_rx__meter"),
            climate: Climate::try_from(i(&m, "climate")).unwrap(),
            n_0: f(&m, "N_0"),
            f_mhz: f(&m, "f__mhz"),
            polarization: Polarization::try_from(i(&m, "pol")).unwrap(),
            epsilon: f(&m, "epsilon"),
            sigma: f(&m, "sigma"),
            mdvar: i(&m, "mdvar"),
        };
        let (t, l, s) = (f(&m, "time"), f(&m, "location"), f(&m, "situation"));

        // NTIA PFL-array entry point
        let out = p2p_tls_pfl(pfl, &params, t, l, s).unwrap_or_else(|e| panic!("row {idx}: {e}"));

        // Owned-profile and borrowed-profile entry points must agree bit-for-bit
        let np = pfl[0] as usize;
        let input = P2pInput::new(&params, pfl[2..np + 3].to_vec(), pfl[1]);
        let out2 = p2p_tls(&input, t, l, s).unwrap();
        assert_eq!(out, out2);
        let out3 = p2p_tls_profile(&pfl[2..np + 3], pfl[1], &params, t, l, s).unwrap();
        assert_eq!(out, out3);

        stats.check(&format!("p2p.csv row {idx}"), out.loss_db, m["A__db"]);
        eprintln!(
            "p2p.csv row {idx}: loss {:.6} dB (ref {}), warnings 0x{:04x}, mode {:?}, d {:.3} km",
            out.loss_db, m["A__db"], out.warnings, out.intermediate.mode, out.intermediate.d_km
        );
    }
    eprintln!("p2p.csv: {} vectors, max |err| vs printed reference = {:.6} dB", stats.n, stats.max_err);
}

#[test]
fn area_csv_vectors() {
    let (header, rows) = parse_csv(&read("area.csv"));
    assert_eq!(rows.len(), 5);
    let mut stats = Stats::default();
    for (idx, row) in rows.iter().enumerate() {
        let m = row_map(&header, row);
        let input = AreaInput {
            h_tx_m: f(&m, "h_tx__meter"),
            h_rx_m: f(&m, "h_rx__meter"),
            tx_siting: SitingCriteria::try_from(i(&m, "tx_siting_criteria")).unwrap(),
            rx_siting: SitingCriteria::try_from(i(&m, "rx_siting_criteria")).unwrap(),
            d_km: f(&m, "d__km"),
            delta_h_m: f(&m, "delta_h__meter"),
            climate: Climate::try_from(i(&m, "climate")).unwrap(),
            n_0: f(&m, "N_0"),
            f_mhz: f(&m, "f__mhz"),
            polarization: Polarization::try_from(i(&m, "pol")).unwrap(),
            epsilon: f(&m, "epsilon"),
            sigma: f(&m, "sigma"),
            mdvar: i(&m, "mdvar"),
        };
        let out = area_tls(&input, f(&m, "time"), f(&m, "location"), f(&m, "situation"))
            .unwrap_or_else(|e| panic!("row {idx}: {e}"));
        stats.check(&format!("area.csv row {idx}"), out.loss_db, m["A__db"]);
        eprintln!(
            "area.csv row {idx}: loss {:.6} dB (ref {}), warnings 0x{:04x}, mode {:?}",
            out.loss_db, m["A__db"], out.warnings, out.intermediate.mode
        );
    }
    eprintln!("area.csv: {} vectors, max |err| vs printed reference = {:.6} dB", stats.n, stats.max_err);
}

// ---------------------------------------------------------------------------
// cmd_examples (ITMDrvr input/output files)
// ---------------------------------------------------------------------------

/// Parse an ITMDrvr input file (`key,value[,value...]` per line).
fn parse_input(rel: &str) -> HashMap<String, Vec<f64>> {
    read(rel)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut it = l.split(',');
            let k = it.next().unwrap().trim().to_string();
            let v = it.map(|v| v.trim().parse::<f64>().unwrap()).collect();
            (k, v)
        })
        .collect()
}

/// Parsed ITMDrvr output file.
struct DrvrOutput {
    /// `label -> first value token` for every "label   value  (unit)" line.
    kv: HashMap<String, String>,
    /// Rows of the results table, if any (all numeric columns).
    table: Vec<Vec<String>>,
}

fn parse_output(rel: &str) -> DrvrOutput {
    let text = read(rel);
    let mut kv = HashMap::new();
    let mut table = Vec::new();
    let mut in_table = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            in_table = false;
            continue;
        }
        if line.starts_with("Basic Transmission Loss Results") {
            in_table = true;
            continue;
        }
        if in_table {
            let toks: Vec<String> = line.split_whitespace().map(str::to_string).collect();
            if toks.iter().all(|t| t.parse::<f64>().is_ok()) {
                table.push(toks);
            }
            continue;
        }
        // labels are left-aligned in a 25-char column
        if line.len() > 25 {
            let (label, rest) = line.split_at(25);
            if let Some(v) = rest.split_whitespace().next() {
                kv.insert(label.trim().to_string(), v.to_string());
            }
        }
    }
    DrvrOutput { kv, table }
}

fn load_pfl_txt() -> Vec<f64> {
    read("cmd_examples/pfl.txt")
        .split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.parse().unwrap())
        .collect()
}

fn p2p_params_from(inp: &HashMap<String, Vec<f64>>) -> P2pParams {
    P2pParams {
        h_tx_m: inp["h_tx__meter"][0],
        h_rx_m: inp["h_rx__meter"][0],
        climate: Climate::try_from(inp["climate"][0] as i32).unwrap(),
        n_0: inp["N_0"][0],
        f_mhz: inp["f__mhz"][0],
        polarization: Polarization::try_from(inp["pol"][0] as i32).unwrap(),
        epsilon: inp["epsilon"][0],
        sigma: inp["sigma"][0],
        mdvar: inp["mdvar"][0] as i32,
    }
}

fn area_input_from(inp: &HashMap<String, Vec<f64>>, d_km: f64) -> AreaInput {
    AreaInput {
        h_tx_m: inp["h_tx__meter"][0],
        h_rx_m: inp["h_rx__meter"][0],
        tx_siting: SitingCriteria::try_from(inp["tx_site_criteria"][0] as i32).unwrap(),
        rx_siting: SitingCriteria::try_from(inp["rx_site_criteria"][0] as i32).unwrap(),
        d_km,
        delta_h_m: inp["delta_h__meter"][0],
        climate: Climate::try_from(inp["climate"][0] as i32).unwrap(),
        n_0: inp["N_0"][0],
        f_mhz: inp["f__mhz"][0],
        polarization: Polarization::try_from(inp["pol"][0] as i32).unwrap(),
        epsilon: inp["epsilon"][0],
        sigma: inp["sigma"][0],
        mdvar: inp["mdvar"][0] as i32,
    }
}

fn check_warnings(name: &str, out: &DrvrOutput, actual: u32) {
    let w = &out.kv["ITM Warning Flags"];
    let expected = u32::from_str_radix(w.trim_start_matches("0x"), 16).unwrap();
    assert_eq!(actual, expected, "{name}: warnings 0x{actual:04x} vs reference {w}");
    let rc: i32 = out.kv["ITM Return Code"].parse().unwrap();
    let my_rc = if actual != 0 { SUCCESS_WITH_WARNINGS } else { SUCCESS };
    assert_eq!(my_rc, rc, "{name}: return code");
}

fn check_intermediate(name: &str, out: &DrvrOutput, r: &ItmOutput, stats: &mut Stats) {
    let iv = &r.intermediate;
    let kv = &out.kv;
    stats.check(&format!("{name} Free Space"), iv.a_fs_db, &kv["Free Space"]);
    stats.check(&format!("{name} d__km"), iv.d_km, &kv["d__km"]);
    stats.check(&format!("{name} theta_hzn_tx (mrad)"), iv.theta_hzn[0] * 1000.0, &kv["theta_hzn_tx"]);
    stats.check(&format!("{name} theta_hzn_rx (mrad)"), iv.theta_hzn[1] * 1000.0, &kv["theta_hzn_rx"]);
    // horizon distances are printed as whole meters; compare at that precision (not counted in dB stats)
    for (k, v) in [("d_hzn_tx__meter", iv.d_hzn_m[0]), ("d_hzn_rx__meter", iv.d_hzn_m[1])] {
        let e: f64 = kv[k].parse().unwrap();
        assert!((v - e).abs() <= 0.5 + 1e-9, "{name} {k}: {v} vs {e}");
    }
    stats.check(&format!("{name} h_e_tx"), iv.h_e_m[0], &kv["h_e_tx__meter"]);
    stats.check(&format!("{name} h_e_rx"), iv.h_e_m[1], &kv["h_e_rx__meter"]);
    stats.check(&format!("{name} N_s"), iv.n_s, &kv["N_s"]);
    stats.check(&format!("{name} delta_h"), iv.delta_h_m, &kv["delta_h__meter"]);
    stats.check(&format!("{name} A_ref"), iv.a_ref_db, &kv["A_ref__db"]);
    let mode: i32 = kv["Mode of Propagation"].parse().unwrap();
    assert_eq!(iv.mode.as_i32(), mode, "{name}: mode of propagation");
}

#[test]
fn cmd_example_p2ptls() {
    let inp = parse_input("cmd_examples/i_p2ptls.txt");
    let out = parse_output("cmd_examples/o_p2ptls.txt");
    let pfl = load_pfl_txt();
    let r = p2p_tls_pfl(&pfl, &p2p_params_from(&inp), inp["time"][0], inp["location"][0], inp["situation"][0]).unwrap();
    let mut stats = Stats::default();
    check_warnings("p2ptls", &out, r.warnings);
    stats.check("p2ptls loss", r.loss_db, &out.kv["Basic Transmission Loss"]);
    check_intermediate("p2ptls", &out, &r, &mut stats);
    eprintln!("cmd p2ptls: {} values, max |err| {:.6}", stats.n, stats.max_err);
}

#[test]
fn cmd_example_p2pcr() {
    let inp = parse_input("cmd_examples/i_p2pcr.txt");
    let out = parse_output("cmd_examples/o_p2pcr.txt");
    let pfl = load_pfl_txt();
    let r = p2p_cr_pfl(&pfl, &p2p_params_from(&inp), inp["confidence"][0], inp["reliability"][0]).unwrap();
    let mut stats = Stats::default();
    check_warnings("p2pcr", &out, r.warnings);
    stats.check("p2pcr loss", r.loss_db, &out.kv["Basic Transmission Loss"]);
    check_intermediate("p2pcr", &out, &r, &mut stats);
    eprintln!("cmd p2pcr: {} values, max |err| {:.6}", stats.n, stats.max_err);
}

#[test]
fn cmd_example_p2pcr_tbl() {
    let inp = parse_input("cmd_examples/i_p2pcr_tbl.txt");
    let out = parse_output("cmd_examples/o_p2pcr_tbl.txt");
    let pfl = load_pfl_txt();
    let params = p2p_params_from(&inp);
    let confs = &inp["confidence"];
    let rels = &inp["reliability"];
    // the first numeric line of the table is the confidence header row
    let rows: Vec<&Vec<String>> = out.table.iter().filter(|r| r.len() == confs.len() + 1).collect();
    assert_eq!(rows.len(), rels.len());
    let mut stats = Stats::default();
    let mut last = None;
    for row in rows {
        let rel: f64 = row[0].parse().unwrap();
        assert!(rels.contains(&rel));
        assert_eq!(row.len(), confs.len() + 1);
        for (j, &conf) in confs.iter().enumerate() {
            let r = p2p_cr_pfl(&pfl, &params, conf, rel).unwrap();
            check_warnings("p2pcr_tbl", &out, r.warnings);
            stats.check(&format!("p2pcr_tbl rel {rel} conf {conf}"), r.loss_db, &row[j + 1]);
            last = Some(r);
        }
    }
    check_intermediate("p2pcr_tbl", &out, &last.unwrap(), &mut stats);
    eprintln!("cmd p2pcr_tbl: {} values, max |err| {:.6}", stats.n, stats.max_err);
}

#[test]
fn cmd_example_areatls() {
    let inp = parse_input("cmd_examples/i_areatls.txt");
    let out = parse_output("cmd_examples/o_areatls.txt");
    let r = area_tls(&area_input_from(&inp, inp["d__km"][0]), inp["time"][0], inp["location"][0], inp["situation"][0])
        .unwrap();
    let mut stats = Stats::default();
    check_warnings("areatls", &out, r.warnings);
    stats.check("areatls loss", r.loss_db, &out.kv["Basic Transmission Loss"]);
    check_intermediate("areatls", &out, &r, &mut stats);
    eprintln!("cmd areatls: {} values, max |err| {:.6}", stats.n, stats.max_err);
}

#[test]
fn cmd_example_areacr() {
    let inp = parse_input("cmd_examples/i_areacr.txt");
    let out = parse_output("cmd_examples/o_areacr.txt");
    let r = area_cr(&area_input_from(&inp, inp["d__km"][0]), inp["confidence"][0], inp["reliability"][0]).unwrap();
    let mut stats = Stats::default();
    check_warnings("areacr", &out, r.warnings);
    stats.check("areacr loss", r.loss_db, &out.kv["Basic Transmission Loss"]);
    check_intermediate("areacr", &out, &r, &mut stats);
    eprintln!("cmd areacr: {} values, max |err| {:.6}", stats.n, stats.max_err);
}

#[test]
fn cmd_example_areacr_tbl() {
    let inp = parse_input("cmd_examples/i_areacr_tbl.txt");
    let out = parse_output("cmd_examples/o_areacr_tbl.txt");
    let confs = &inp["confidence"];
    let rel = inp["reliability"][0];
    assert_eq!(out.table.len(), 19);
    let mut stats = Stats::default();
    for row in &out.table {
        let d_km: f64 = row[0].parse().unwrap();
        assert_eq!(row.len(), confs.len() + 2);
        for (j, &conf) in confs.iter().enumerate() {
            let r = area_cr(&area_input_from(&inp, d_km), conf, rel).unwrap();
            check_warnings("areacr_tbl", &out, r.warnings);
            if j == 0 {
                stats.check(&format!("areacr_tbl {d_km} km free space"), r.intermediate.a_fs_db, &row[1]);
            }
            stats.check(&format!("areacr_tbl {d_km} km conf {conf}"), r.loss_db, &row[j + 2]);
        }
    }
    eprintln!("cmd areacr_tbl: {} values, max |err| {:.6}", stats.n, stats.max_err);
}
