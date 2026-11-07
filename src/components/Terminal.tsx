import { useEffect, useImperativeHandle, useRef, type Ref } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

export interface TerminalHandle {
  write: (data: string) => void;
  clear: () => void;
}

// React 19: `ref` is a plain prop on function components, so no forwardRef wrapper needed.
export function Terminal({ ref }: { ref: Ref<TerminalHandle> }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);

  // The handle reads xtermRef at *call* time, not render time, so it survives StrictMode's
  // mount→unmount→mount cycle and never writes into a disposed instance captured in a closure.
  useImperativeHandle(ref, () => ({
    write: (data) => xtermRef.current?.write(data),
    clear: () => xtermRef.current?.reset(),
  }), []);

  useEffect(() => {
    const host = hostRef.current!;
    const term = new XTerm({
      convertEol: true, // tools emit bare \n; xterm needs \r\n or lines staircase to the right
      disableStdin: true, // read-only log: the runner's stdin is null anyway
      scrollback: 20_000,
      fontFamily: '"Cascadia Code", "Cascadia Mono", Consolas, monospace',
      fontSize: 12.5,
      lineHeight: 1.25,
      cursorBlink: false,
      cursorInactiveStyle: "none",
      theme: {
        background: "#0c0c0e",
        foreground: "#d4d4d8",
        cursor: "#0c0c0e",
        selectionBackground: "#3f3f4680",
        black: "#18181b",
        brightBlack: "#71717a",
        red: "#f87171",
        green: "#4ade80",
        yellow: "#fbbf24",
        blue: "#60a5fa",
        magenta: "#c084fc",
        cyan: "#22d3ee",
        white: "#e4e4e7",
      },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    term.write("\x1b[?25l"); // hide the cursor; nothing is ever typed here
    fit.fit();
    xtermRef.current = term;

    // Refit on any container size change (window resize, panel layout), not only window resize.
    const ro = new ResizeObserver(() => {
      try {
        fit.fit();
      } catch {
        /* host detached mid-resize */
      }
    });
    ro.observe(host);
    return () => {
      ro.disconnect();
      term.dispose();
      xtermRef.current = null;
    };
  }, []);

  return <div className="terminal-host" ref={hostRef} />;
}
