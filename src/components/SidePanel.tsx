import { coordinateOf } from "../game/orientation";
import type { GameSnapshot, LegalTargetsResult } from "../game/types";
import type { SearchProgressPayload } from "../protocol";

interface SidePanelProps {
  snapshot: GameSnapshot;
  thinking: boolean;
  searchProgress: SearchProgressPayload | null;
  busy: boolean;
  aiVsAi: boolean;
  aiPaused: boolean;
  diagnostics: boolean;
  selectedDiagnostics: LegalTargetsResult | null;
  muted: boolean;
  onNewGame: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onSave: () => void;
  onLoad: () => void;
  onFlip: () => void;
  onDiagnostics: () => void;
  onAiVsAi: () => void;
  onMute: () => void;
  onHowToPlay: () => void;
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat("en-US").format(value);
}

export function SidePanel(props: SidePanelProps) {
  const { snapshot } = props;
  const state = snapshot.gameState;
  const terminal = state.result !== "ongoing";
  const computerSide = snapshot.humanSide === "blue" ? "Red" : "Blue";
  const capabilityLabels = [
    snapshot.runtime.capabilities.scalar ? "scalar" : null,
    snapshot.runtime.capabilities.simd ? "SIMD" : null,
    snapshot.runtime.capabilities.threads ? "threads" : null,
    snapshot.runtime.capabilities.lazySmp ? "Lazy SMP" : null,
    snapshot.runtime.capabilities.workerPoolSize > 1
      ? `${snapshot.runtime.capabilities.workerPoolSize} lanes`
      : null,
  ].filter(Boolean).join(", ");
  const status = terminal
    ? `${state.winner?.toUpperCase() ?? "Game"} wins`
    : props.thinking
      ? `${state.side_to_move === "blue" ? "Blue" : "Red"} AI is thinking...`
      : props.aiPaused && state.side_to_move !== snapshot.humanSide
        ? `${state.side_to_move === "blue" ? "Blue" : "Red"} AI is paused`
      : `${state.side_to_move === "blue" ? "Blue" : "Red"} to move`;

  return (
    <aside className="side-panel" aria-label="Game controls and information">
      <section className={`status-card status-card--${state.side_to_move}`} aria-live="polite">
        <span className="status-dot" />
        <div>
          <small>{terminal ? state.result.replaceAll("_", " ") : "Current turn"}</small>
          <strong>{status}</strong>
          <span>You: {snapshot.humanSide === "blue" ? "Blue" : "Red"} · Computer: {computerSide}</span>
        </div>
      </section>

      <div className="toolbar">
        <button type="button" onClick={props.onNewGame} disabled={props.thinking || props.busy}>New Game</button>
        <button type="button" onClick={props.onUndo} disabled={!snapshot.canUndo || props.thinking || props.busy}>Undo</button>
        <button type="button" onClick={props.onRedo} disabled={!snapshot.canRedo || props.thinking || props.busy}>Redo</button>
        <button type="button" onClick={props.onFlip}>Flip Board</button>
        <button type="button" onClick={props.onSave} disabled={props.thinking || props.busy}>Save</button>
        <button type="button" onClick={props.onLoad} disabled={props.thinking || props.busy}>Load</button>
        <button type="button" onClick={props.onDiagnostics} aria-pressed={props.diagnostics}>Diagnostics</button>
        <button type="button" onClick={props.onAiVsAi} aria-pressed={props.aiVsAi} disabled={terminal}>
          {props.aiVsAi ? "Stop AI vs AI" : props.aiPaused ? "Resume AI vs AI" : "AI vs AI"}
        </button>
        <button type="button" onClick={props.onMute} aria-pressed={props.muted}>{props.muted ? "Unmute" : "Mute"}</button>
        <button type="button" onClick={props.onHowToPlay}>How to Play</button>
      </div>

      {props.thinking ? (
        <section className="info-card ai-card" aria-live="polite">
          <div className="section-heading">
            <h2>AI search</h2>
            <span className="difficulty-pill">{snapshot.difficulty}</span>
          </div>
          <dl className="stats-grid">
            <div><dt>Depth</dt><dd>{props.searchProgress?.depth ?? "-"}</dd></div>
            <div><dt>Time</dt><dd>{props.searchProgress ? `${props.searchProgress.elapsedMs.toFixed(0)} ms` : "Starting"}</dd></div>
            <div><dt>Nodes</dt><dd>{props.searchProgress ? formatNumber(props.searchProgress.nodes) : "-"}</dd></div>
            <div><dt>Score</dt><dd>{props.searchProgress?.score ?? "-"}</dd></div>
          </dl>
        </section>
      ) : null}

      {snapshot.aiStats ? (
        <section className="info-card ai-card">
          <div className="section-heading">
            <h2>Last AI search</h2>
            <span className="difficulty-pill">{snapshot.aiStats.difficulty}</span>
          </div>
          <dl className="stats-grid">
            <div><dt>Depth</dt><dd>{snapshot.aiStats.depth}</dd></div>
            <div><dt>Time</dt><dd>{snapshot.aiStats.elapsedMs.toFixed(0)} ms</dd></div>
            <div><dt>Nodes</dt><dd>{formatNumber(snapshot.aiStats.nodes)}</dd></div>
            <div><dt>Nodes/s</dt><dd>{formatNumber(snapshot.aiStats.nodesPerSecond)}</dd></div>
            <div><dt>TT hits</dt><dd>{formatNumber(snapshot.aiStats.transpositionHits)}</dd></div>
            <div><dt>TB hits</dt><dd>{formatNumber(snapshot.aiStats.tablebaseHits)}</dd></div>
            <div><dt>Lanes</dt><dd>{snapshot.aiStats.workerPoolSize ?? 1}</dd></div>
            <div><dt>Stopped by</dt><dd>{snapshot.aiStats.stopReason}</dd></div>
          </dl>
        </section>
      ) : null}

      <section className="info-card history-card">
        <div className="section-heading">
          <h2>Move history</h2>
          <span>{snapshot.history.length} plies</span>
        </div>
        <ol className="move-history">
          {snapshot.history.length === 0 ? <li className="empty-history">The jungle is quiet.</li> : null}
          {[...snapshot.history].reverse().map((move) => (
            <li key={move.number}>
              <span className={`history-number history-number--${move.piece.side}`}>{move.number}</span>
              <span>
                <strong>{move.pieceName}</strong>
                <small>{coordinateOf(move.origin)} → {coordinateOf(move.destination)}{move.capturedName ? ` · captured ${move.capturedName}` : ""}{move.is_jump ? " · jump" : ""}</small>
              </span>
            </li>
          ))}
        </ol>
      </section>

      {props.diagnostics ? (
        <section className="info-card diagnostics-card">
          <div className="section-heading"><h2>Diagnostics</h2><span>authoritative engine</span></div>
          <dl>
            <div><dt>Revision</dt><dd>{snapshot.revision}</dd></div>
            <div><dt>Selected</dt><dd>{props.selectedDiagnostics ? `${coordinateOf(props.selectedDiagnostics.origin)} (#${props.selectedDiagnostics.origin})` : "None"}</dd></div>
            <div><dt>Effective rank</dt><dd>{props.selectedDiagnostics?.effectiveRank ?? "-"}</dd></div>
            <div><dt>Legal targets</dt><dd>{props.selectedDiagnostics?.targets.map((target) => {
              const kind = target.capture ? "capture" : "move";
              return `${coordinateOf(target.destination)} (${target.jump ? `jump ${kind}` : kind})`;
            }).join(", ") || "-"}</dd></div>
            <div><dt>Engine</dt><dd>{snapshot.runtime.engineName} {snapshot.runtime.engineVersion}</dd></div>
            <div><dt>Search</dt><dd>{snapshot.runtime.searchVersion}</dd></div>
            <div><dt>Capabilities</dt><dd>{capabilityLabels || "baseline"}</dd></div>
            <div><dt>Worker pool</dt><dd>{snapshot.runtime.capabilities.workerPoolSize}</dd></div>
            <div><dt>Cancellation</dt><dd>{snapshot.runtime.capabilities.cancellable ? "Available" : "Between moves"}</dd></div>
            <div><dt>Tablebase</dt><dd>{snapshot.runtime.tablebaseLoaded ? "Loaded" : "Unavailable"}</dd></div>
          </dl>
        </section>
      ) : null}
    </aside>
  );
}
