import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { Artifact } from "../lib/api";
import { formatBytes } from "../lib/format";

export function Artifacts({ items }: { items: Artifact[] }) {
  if (items.length === 0) return null;
  return (
    <section className="card artifacts">
      <h3>Artifacts</h3>
      <ul>
        {items.map((a) => (
          <li key={a.path}>
            <span className="artifact-icon">{a.isDir ? "▣" : kindIcon(a.name)}</span>
            <div className="artifact-body">
              <div className="artifact-name" title={a.path}>{a.name}</div>
              <div className="muted small">{formatBytes(a.size)}{a.isDir ? " · folder" : ""}</div>
            </div>
            <button className="btn btn-ghost small" onClick={() => (a.isDir ? openPath(a.path) : revealItemInDir(a.path))}>
              {a.isDir ? "Open" : "Show"}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

function kindIcon(name: string) {
  const n = name.toLowerCase();
  if (n.endsWith(".msi") || n.includes("setup")) return "⬇";
  if (n.endsWith(".exe")) return "▶";
  return "◆";
}
