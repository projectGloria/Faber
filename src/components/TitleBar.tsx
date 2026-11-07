import { getCurrentWindow } from "@tauri-apps/api/window";

const win = getCurrentWindow();

/** Frameless window chrome: the bar itself is the drag handle; buttons opt out by not carrying the attribute. */
export function TitleBar({ children }: { children?: React.ReactNode }) {
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <span className="brand-mark">F</span>
        <span className="brand-name" data-tauri-drag-region>Faber</span>
      </div>
      <div className="titlebar-center" data-tauri-drag-region>{children}</div>
      <div className="window-controls">
        <button aria-label="Minimize" onClick={() => win.minimize()}>
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" /></svg>
        </button>
        <button aria-label="Maximize" onClick={() => win.toggleMaximize()}>
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" /></svg>
        </button>
        <button aria-label="Close" className="close" onClick={() => win.close()}>
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" /></svg>
        </button>
      </div>
    </header>
  );
}
