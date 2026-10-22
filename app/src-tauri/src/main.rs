#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pathless_engine::antennas::{AntennaLibrary, AntennaSummary};
use pathless_engine::coverage::{CoverageGrid, CoverageSummary};
use pathless_engine::dataset::{DataPlan, DataStatus, Dataset};
use pathless_engine::link::LinkReport;
use pathless_engine::project::Project;
use pathless_engine::{coverage, dataset, link, DataManager, Progress};
use pathless_rf::antenna::AntennaPattern;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    data: Arc<DataManager>,
    dataset: Mutex<Option<Arc<Dataset>>>,
    coverage: Mutex<Option<Arc<CoverageGrid>>>,
    antennas: Arc<Mutex<AntennaLibrary>>,
}

impl AppState {
    fn dataset(&self) -> Result<Arc<Dataset>, String> {
        self.dataset.lock().unwrap().clone().ok_or_else(|| "No terrain loaded — fetch data first".to_string())
    }
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

fn progress_emitter(app: &AppHandle) -> impl Fn(Progress) + Send + Sync + Clone + 'static {
    let app = app.clone();
    move |p: Progress| {
        let _ = app.emit("progress", p);
    }
}

#[tauri::command]
async fn plan_data(state: State<'_, AppState>, project: Project) -> CmdResult<DataPlan> {
    dataset::plan(&state.data, &project).map_err(err)
}

#[tauri::command]
async fn fetch_data(app: AppHandle, state: State<'_, AppState>, project: Project) -> CmdResult<DataStatus> {
    let ds = dataset::fetch(&state.data, &project, progress_emitter(&app)).await.map_err(err)?;
    let status = ds.status();
    *state.dataset.lock().unwrap() = Some(Arc::new(ds));
    *state.coverage.lock().unwrap() = None;
    Ok(status)
}

#[tauri::command]
fn data_status(state: State<'_, AppState>) -> Option<DataStatus> {
    state.dataset.lock().unwrap().as_ref().map(|d| d.status())
}

#[tauri::command]
fn elevation_at(state: State<'_, AppState>, lat: f64, lon: f64) -> Option<f64> {
    state.dataset.lock().unwrap().as_ref().and_then(|d| d.elevation_at(lat, lon))
}

#[tauri::command]
async fn analyze_link(state: State<'_, AppState>, project: Project, link_id: String) -> CmdResult<LinkReport> {
    let ds = state.dataset()?;
    let antennas = state.antennas.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lib = antennas.lock().unwrap();
        link::analyze(&project, &link_id, &ds, &lib)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn run_coverage(app: AppHandle, state: State<'_, AppState>, project: Project) -> CmdResult<CoverageSummary> {
    let ds = state.dataset()?;
    let antennas = state.antennas.clone();
    let progress = progress_emitter(&app);
    let (grid, summary) = tauri::async_runtime::spawn_blocking(move || {
        let lib = antennas.lock().unwrap().clone();
        coverage::run(&project, &ds, &lib, progress)
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    *state.coverage.lock().unwrap() = Some(Arc::new(grid));
    Ok(summary)
}

#[tauri::command]
fn coverage_value_at(state: State<'_, AppState>, lat: f64, lon: f64) -> Option<f64> {
    state.coverage.lock().unwrap().as_ref().and_then(|g| g.value_at(lat, lon))
}

#[tauri::command]
fn antenna_catalog(state: State<'_, AppState>) -> Vec<AntennaSummary> {
    state.antennas.lock().unwrap().list()
}

#[tauri::command]
fn antenna_pattern(state: State<'_, AppState>, id: String, frequency_mhz: f64) -> CmdResult<AntennaPattern> {
    state.antennas.lock().unwrap().pattern(Some(&id), frequency_mhz).map_err(err)
}

#[tauri::command]
fn import_antenna(state: State<'_, AppState>, path: PathBuf) -> CmdResult<AntennaSummary> {
    state.antennas.lock().unwrap().import(&path).map_err(err)
}

#[tauri::command]
fn save_project(path: PathBuf, project: Project) -> CmdResult<()> {
    let json = serde_json::to_string_pretty(&project).map_err(err)?;
    std::fs::write(&path, json).map_err(err)
}

#[tauri::command]
fn load_project(path: PathBuf) -> CmdResult<Project> {
    let text = std::fs::read_to_string(&path).map_err(err)?;
    serde_json::from_str(&text).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheInfo {
    bytes: u64,
    dir: String,
}

#[tauri::command]
fn cache_info(state: State<'_, AppState>) -> CacheInfo {
    CacheInfo { bytes: state.data.cache_size_bytes(), dir: state.data.cache_dir().display().to_string() }
}

#[tauri::command]
fn clear_cache(state: State<'_, AppState>) -> CmdResult<()> {
    state.data.clear_cache().map_err(err)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data = DataManager::new(None)?;
            let user_antennas = app.path().app_data_dir()?.join("antennas");
            let antennas = AntennaLibrary::load(Some(user_antennas))?;
            app.manage(AppState {
                data: Arc::new(data),
                dataset: Mutex::new(None),
                coverage: Mutex::new(None),
                antennas: Arc::new(Mutex::new(antennas)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            plan_data,
            fetch_data,
            data_status,
            elevation_at,
            analyze_link,
            run_coverage,
            coverage_value_at,
            antenna_catalog,
            antenna_pattern,
            import_antenna,
            save_project,
            load_project,
            cache_info,
            clear_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PathLess");
}
