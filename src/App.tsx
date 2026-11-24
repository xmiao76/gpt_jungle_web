import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Board } from "./components/Board";
import { HowToPlay } from "./components/HowToPlay";
import { Modal } from "./components/Modal";
import { Setup, defaults } from "./components/Setup";
import { ReleaseInfo, download } from "./components/ReleaseInfo";
import {
  apply,
  fresh,
  legal,
  piece,
  readSave,
  type Match,
  type Options,
  type Ply,
} from "./game/match";
import { Runtime, type Choice } from "./game/runtime";
import { PIECE_NAMES } from "./game/types";
import { preloadArtwork } from "./assets";
import { jungleAudio } from "./audio";
import meta from "../generated/metadata.json";
const title = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
const reasons: Record<string, string> = {
  den: "The opposing den was reached.",
  elimination: "Every opposing animal was captured.",
  immobilization: "The opponent has no legal move.",
  repetition: "The same board and side to move occurred three times.",
  "no-capture": "One hundred consecutive plies passed without a capture.",
};

export default function App() {
  const [runtime, setRuntime] = useState<Runtime | null>(null),
    [progress, setProgress] = useState(5),
    [error, setError] = useState("");
  const [line, setLine] = useState<Match[]>([]),
    [cursor, setCursor] = useState(0),
    [options, setOptions] = useState<Options>(defaults);
  const [setup, setSetup] = useState(true),
    [started, setStarted] = useState(false),
    [help, setHelp] = useState(false),
    [review, setReview] = useState(false);
  const [paused, setPaused] = useState(false),
    [flipped, setFlipped] = useState(false),
    [muted, setMuted] = useState(false),
    [selected, select] = useState<number | null>(null);
  const [stats, setStats] = useState<
      (Choice & { elapsed: number; started: number }) | null
    >(null),
    [diagnostics, setDiagnostics] = useState(false);
  const upload = useRef<HTMLInputElement>(null),
    lastUndo = useRef(1);
  const state = line[cursor];
  const moves = useMemo(() => (state ? legal(state) : []), [state]);
  useEffect(() => {
    let live = true;
    const rt = new Runtime();
    const load = async () => {
      await preloadArtwork((fraction) => {
        if (live) setProgress(Math.round(5 + fraction * 55));
      });
      if (!live) return;
      await rt.start(() => {
        if (live) setProgress(85);
      });
    };
    load()
      .then(() => {
        if (live) {
          setLine([fresh("red")]);
          setProgress(100);
          setRuntime(rt);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
      rt.close();
    };
  }, []);
  const aiTurn =
    !!state &&
    started &&
    !setup &&
    !help &&
    !paused &&
    state.status === "playing" &&
    (options.auto || options.humanSide !== state.toPlay);
  useEffect(() => {
    if (!runtime || !state || !aiTurn) return;
    let live = true;
    runtime
      .move(state, options.difficulty)
      .then((result) => {
        if (!live || !result) return;
        const next = apply(state, result.move);
        setLine((prev) => [...prev.slice(0, cursor + 1), next]);
        setCursor(cursor + 1);
        select(null);
        setStats({
          ...result.choice,
          elapsed: result.elapsed,
          started: result.started,
        });
        jungleAudio.play(
          next.status !== "playing"
            ? "win"
            : state.board[result.move.dst]
              ? "capture"
              : "move",
        );
      })
      .catch((e) => {
        if (live) {
          setError(String(e));
          setPaused(true);
        }
      });
    return () => {
      live = false;
      runtime.cancel();
    };
  }, [aiTurn, cursor, options.difficulty, runtime, state]);
  useLayoutEffect(() => {
    if (stats)
      performance.measure("jungle.ai." + options.difficulty, {
        start: stats.started,
        end: performance.now(),
      });
  }, [stats, options.difficulty]);
  if (!state || !runtime)
    return (
      <main className="loading-screen">
        <div className="loading-emblem">鬥</div>
        <h1>Entering the jungle</h1>
        <p>
          {error ||
            (progress < 60
              ? "Preparing the board…"
              : "Starting the local engine…")}
        </p>
        <div
          className="progress-track"
          role="progressbar"
          aria-label="Loading game"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={progress}
        >
          <span style={{ width: progress + "%" }} />
        </div>
        {error && <button onClick={() => location.reload()}>Retry</button>}
      </main>
    );
  const stop = () => {
    runtime.cancel();
    select(null);
    setStats(null);
    setReview(false);
    setError("");
  };
  const start = (o: Options) => {
    stop();
    setOptions(o);
    setLine([fresh(o.firstSide)]);
    setCursor(0);
    setSetup(false);
    setStarted(true);
    setPaused(false);
    setFlipped(false);
  };
  const undo = () => {
    stop();
    const n =
      !options.auto && state.toPlay === options.humanSide && cursor >= 2
        ? 2
        : 1;
    lastUndo.current = n;
    setCursor(Math.max(0, cursor - n));
    setPaused(
      options.auto || (cursor === 1 && state.firstSide !== options.humanSide),
    );
  };
  const play = (m: Ply) => {
    const n = apply(state, m);
    jungleAudio.play(
      n.status !== "playing" ? "win" : state.board[m.dst] ? "capture" : "move",
    );
    setLine([...line.slice(0, cursor + 1), n]);
    setCursor(cursor + 1);
    select(null);
    setStats(null);
    setReview(false);
    setPaused(false);
  };
  const square = (i: number) => {
    const m = moves.find((m) => m.src === selected && m.dst === i);
    if (m) play(m);
    else
      select(
        i !== selected && piece(state.board[i])?.side === options.humanSide
          ? i
          : null,
      );
  };
  const last = state.history.at(-1),
    old = cursor > 0 ? line[cursor - 1] : undefined;
  const captured = last && old ? piece(old.board[last.dst]) : null;
  const terminal = state.status !== "playing",
    result = state.winner ? title(state.winner) + " wins" : "Draw";
  const selectedMoves = moves.filter((m) => m.src === selected);
  const board = {
    gameState: { board: state.board.map(piece) },
    lastMove: last ? { origin: last.src, destination: last.dst } : null,
  };
  return (
    <div
      className="app-shell"
      onPointerDown={() => jungleAudio.unlock()}
      onKeyDown={() => jungleAudio.unlock()}
    >
      <header className="app-header">
        <div className="brand-lockup">
          <div className="brand-mark">鬥</div>
          <div>
            <span className="eyebrow">Dou Shou Qi</span>
            <h1>Jungle</h1>
          </div>
        </div>
        <span className="runtime-badge">
          <i className="runtime-light" />
          {meta.title}
        </span>
      </header>
      <main
        className="game-layout"
        data-revision={state.rev}
        data-side={state.toPlay}
        data-status={state.status}
        data-runtime={runtime.mode}
        data-reason={state.reason}
      >
        <section className="board-column" aria-label="Game board">
          <Board
            snapshot={board}
            captured={
              captured && last
                ? { piece: captured, square: last.dst, rev: state.rev }
                : null
            }
            flipped={flipped}
            selected={selected}
            onSquare={square}
            disabled={
              terminal ||
              options.auto ||
              aiTurn ||
              state.toPlay !== options.humanSide
            }
            targets={selectedMoves.map((m) => ({
              destination: m.dst,
              capture: !!state.board[m.dst],
              captured: null,
              jump:
                Math.abs(m.src - m.dst) > 7 || Math.abs(m.src - m.dst) === 3,
              note: "",
            }))}
          />
          <div className="board-meta">
            <span>{title(state.firstSide)} first</span>
            <span>{title(options.difficulty)} AI</span>
            <span>{state.noCapturePlies} quiet plies</span>
          </div>
        </section>
        <aside
          className="side-panel"
          aria-label="Game controls and information"
        >
          <section
            className={"status-card status-card--" + state.toPlay}
            aria-live="polite"
          >
            <i className="status-dot" />
            <div>
              <small>
                {options.auto
                  ? "AI vs AI"
                  : "You play " + title(options.humanSide)}
              </small>
              <strong>
                {terminal
                  ? result
                  : aiTurn
                    ? title(state.toPlay) + " is thinking…"
                    : paused
                      ? "Game paused"
                      : title(state.toPlay) + " to move"}
              </strong>
              <span>
                {terminal
                  ? reasons[state.reason]
                  : selected !== null
                    ? selectedMoves.length + " legal destinations"
                    : "Ply " + state.rev}
              </span>
            </div>
          </section>
          <div className="toolbar">
            <button
              onClick={() => {
                stop();
                setSetup(true);
              }}
            >
              New game
            </button>
            <button onClick={() => start(options)}>Restart</button>
            <button onClick={undo} disabled={!cursor}>
              Undo
            </button>
            <button
              disabled={cursor >= line.length - 1}
              onClick={() => {
                stop();
                setCursor(Math.min(line.length - 1, cursor + lastUndo.current));
                setPaused(options.auto);
              }}
            >
              Redo
            </button>
            <button onClick={() => setFlipped(!flipped)}>Flip board</button>
            <button
              onClick={() =>
                download(
                  JSON.stringify(
                    {
                      schema: "gpt-jungle-save",
                      version: 2,
                      rulesId: meta.rulesId,
                      settings: options,
                      flipped,
                      history: state.history,
                    },
                    null,
                    2,
                  ),
                  "jungle-save.json",
                  "application/json",
                )
              }
            >
              Save
            </button>
            <button onClick={() => upload.current?.click()}>Load</button>
            <button
              aria-pressed={paused}
              onClick={() => {
                runtime.cancel();
                setPaused(!paused);
              }}
              disabled={terminal}
            >
              {paused ? "Resume" : "Pause"}
            </button>
            <button
              aria-pressed={muted}
              onClick={() => {
                setMuted(!muted);
                jungleAudio.setMuted(!muted);
              }}
            >
              {muted ? "Unmute" : "Mute"}
            </button>
            <button onClick={() => setHelp(true)}>How to play</button>
            <button
              aria-pressed={diagnostics}
              onClick={() => setDiagnostics(!diagnostics)}
            >
              Diagnostics
            </button>
          </div>
          {diagnostics && (
            <section className="info-card">
              <h2>Engine</h2>
              <p>
                {meta.runtime} · {runtime.mode} · revision {state.rev}
              </p>
              {stats && (
                <p>
                  Depth {stats.depth} · {stats.nodes.toLocaleString()} nodes ·{" "}
                  {stats.elapsed.toFixed(1)} ms
                </p>
              )}
            </section>
          )}
          <section className="info-card history-card">
            <div className="section-heading">
              <h2>Move history</h2>
              <span>{cursor} plies</span>
            </div>
            <ol className="move-history">
              {!cursor && (
                <li className="empty-history">The jungle is quiet.</li>
              )}
              {state.history
                .map((m, i) => {
                  const p = piece(line[i].board[m.src])!,
                    victim = piece(line[i].board[m.dst]);
                  return (
                    <li key={i}>
                      <span
                        className={"history-number history-number--" + p.side}
                      >
                        {i + 1}
                      </span>
                      <span>
                        <strong>{PIECE_NAMES[p.kind]}</strong>
                        <small>
                          {m.src} → {m.dst}
                          {victim && " · captured " + PIECE_NAMES[victim.kind]}
                        </small>
                      </span>
                    </li>
                  );
                })
                .reverse()}
            </ol>
          </section>
          <section className="info-card">
            <ReleaseInfo />
          </section>
        </aside>
      </main>
      <input
        type="file"
        accept=".json,application/json"
        className="visually-hidden"
        ref={upload}
        onChange={async (e) => {
          const f = e.target.files?.[0];
          e.target.value = "";
          if (!f) return;
          try {
            const data = readSave(await f.text());
            stop();
            setLine(data.line);
            setCursor(data.line.length - 1);
            setOptions(data.options);
            setFlipped(data.flipped);
            setPaused(data.options.auto);
            setSetup(false);
            setStarted(true);
          } catch (err) {
            setError(String(err));
          }
        }}
      />
      {setup && !help && (
        <Setup
          initial={options}
          close={started ? () => setSetup(false) : undefined}
          start={start}
          help={() => setHelp(true)}
        />
      )}
      {help && <HowToPlay onClose={() => setHelp(false)} />}
      {terminal && !review && !setup && !help && (
        <Modal title={result}>
          <div className="result-content">
            <div className="result-orb">✦</div>
            <p>{reasons[state.reason]}</p>
            <div className="modal-actions">
              <button className="primary-button" onClick={() => setSetup(true)}>
                New game
              </button>
              <button
                className="secondary-button"
                onClick={() => setReview(true)}
              >
                Review board
              </button>
            </div>
          </div>
        </Modal>
      )}
      {error && (
        <div className="toast toast--error" role="alert">
          {error}
          <button onClick={() => setError("")}>Dismiss</button>
        </div>
      )}
    </div>
  );
}
