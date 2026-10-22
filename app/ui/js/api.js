// Thin wrapper over the Tauri IPC bridge (window.__TAURI__ is injected because
// tauri.conf.json sets app.withGlobalTauri = true).

const T = window.__TAURI__;

export function hasBackend() {
  return !!(T && T.core);
}

export async function call(cmd, args = {}) {
  if (!hasBackend()) throw new Error(`Backend unavailable (${cmd}) — run inside the PathLess app`);
  try {
    return await T.core.invoke(cmd, args);
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : (e && e.message) || JSON.stringify(e));
  }
}

export async function listen(event, handler) {
  if (!hasBackend()) return () => {};
  return T.event.listen(event, (e) => handler(e.payload));
}

export async function openFileDialog(opts) {
  if (!hasBackend()) return null;
  return T.dialog.open(opts);
}

export async function saveFileDialog(opts) {
  if (!hasBackend()) return null;
  return T.dialog.save(opts);
}
