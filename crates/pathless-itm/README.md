# pathless-itm

A pure-Rust port of the **NTIA/ITS Irregular Terrain Model (ITM, Longley-Rice)**
reference implementation, v1.x: <https://github.com/NTIA/itm>.

The original C++ code and its test data were written by the U.S. Department of
Commerce, National Telecommunications and Information Administration (NTIA),
Institute for Telecommunication Sciences (ITS). As a work of the U.S. Government
they are in the public domain. This crate ports that implementation function by
function (the module names match the C++ files). It keeps the constants, branch
logic, warning flags and error codes. Credit for the model and the reference
code goes to NTIA/ITS and to the original authors, A. G. Longley, P. L. Rice
and G. A. Hufford.

## Usage

```rust
use pathless_itm::*;

let params = P2pParams {
    h_tx_m: 30.0, h_rx_m: 2.0,
    climate: Climate::ContinentalTemperate, n_0: 301.0, f_mhz: 450.0,
    polarization: Polarization::Vertical,
    epsilon: ground::AVERAGE.epsilon, sigma: ground::AVERAGE.sigma,
    mdvar: ModeOfVariability::Broadcast.mdvar(false, false),
};
// evenly spaced elevations (m AMSL) from TX to RX inclusive, 50 m apart
let out = p2p_tls_profile(&elevations, 50.0, &params, 50.0, 50.0, 50.0)?;
println!("{:.1} dB ({:?})", out.loss_db, out.intermediate.mode);
```

Entry points: `p2p_tls`, `p2p_cr` (owned `P2pInput`), `p2p_tls_profile` and
`p2p_cr_profile` (borrowed slice, no allocation, for hot loops), `p2p_tls_pfl`
and `p2p_cr_pfl` (NTIA `pfl` array), `area_tls` and `area_cr`. There are also
helpers: `free_space_loss_db` and `compute_delta_h_m`.

## Validation

`cargo test -p pathless-itm` runs:

* every NTIA test vector in `tests/data/`: `p2p.csv`/`pfls.csv`, `area.csv` and
  the ITMDrvr `cmd_examples` inputs and outputs. These check losses, warnings,
  return codes and intermediate values to the precision the reference prints
  them at.
* the function-level GoogleTest vectors from the upstream
  `CrossPlatform-and-PropLib2.0` branch (`src/reference_tests.rs`), which agree
  to within about 1e-15 relative error.

## Deviations from the C++

* Profiles are validated (at least 2 points, finite spacing > 0, a well-formed
  `pfl` array). If a profile fails, the crate returns
  `ItmError::InvalidProfile` (code 1100, which is specific to this crate). The
  C++ would read out of bounds instead. This check runs after the NTIA input
  checks, so NTIA errors keep their precedence.
* Climate, polarization and siting criteria are Rust enums, so errors 1002, 1010,
  1019 and 1020 cannot occur through the typed API. `TryFrom<i32>` returns
  them for raw values.
* `ComputeDeltaH` uses stack buffers instead of a `std::vector`. The k-th
  largest value is the same, so the results are identical.
