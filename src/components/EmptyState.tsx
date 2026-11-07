import { basename } from "../lib/format";

interface Props {
  recents: string[];
  onPick: () => void;
  onOpen: (path: string) => void;
  onForget: (path: string) => void;
  dragging: boolean;
}

export function EmptyState({ recents, onPick, onOpen, onForget, dragging }: Props) {
  return (
    <div className={`empty ${dragging ? "dragging" : ""}`}>
      <div className="dropzone">
        <div className="dropzone-mark">F</div>
        <h1>Drop a project folder</h1>
        <p className="muted">Faber detects the stack, installs what's missing, and builds it.</p>
        <button className="btn btn-primary" onClick={onPick}>Choose folder</button>
      </div>
      {recents.length > 0 && (
        <div className="recents">
          <h3>Recent</h3>
          <ul>
            {recents.map((r) => (
              <li key={r}>
                <button className="recent" onClick={() => onOpen(r)} title={r}>
                  <span className="recent-name">{basename(r)}</span>
                  <span className="recent-path muted">{r}</span>
                </button>
                <button className="forget" aria-label={`Forget ${r}`} onClick={() => onForget(r)}>×</button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
