import { useCallback, useEffect, useRef, useState } from "react";
import packageJson from "../package.json";
import { jungleAudio } from "./audio";
import { Board } from "./components/Board";
import { HowToPlay } from "./components/HowToPlay";
import { Modal } from "./components/Modal";
import { NewGameDialog } from "./components/NewGameDialog";
import { SidePanel } from "./components/SidePanel";
import { createSave, downloadSave, parseSave } from "./game/save";
import { undoPlyCount } from "./game/session";
import type {
  Difficulty,
  GameSnapshot,
  JungleSave,
  LegalTarget,
  LegalTargetsResult,
  Side,
} from "./game/types";
import type { ProgressPayload, SearchProgressPayload } from "./protocol";
import { GameWorkerClient, GameWorkerError } from "./worker/client";

type AppPhase = "loading" | "ready" | "error";

const DEFAULT_PROGRESS: ProgressPayload = {
  stage: "runtime",
  percent: 3,
  message: "Preparing the jungle...",
};

function readBooleanPreference(key: string, fallback: boolean): boolean {
  try {
    const value = localStorage.getItem(key);
    return value === null ? fallback : value === "true";
  } catch {
    return fallback;
  }
}

function errorMessage(reason: unknown): string {
  if (reason instanceof GameWorkerError || reason instanceof Error) return reason.message;
  return String(reason);
}

function isSearchCancelled(reason: unknown): boolean {
  return reason instanceof GameWorkerError && reason.code === "SEARCH_CANCELLED";
}

function cueForSnapshot(snapshot: GameSnapshot): "move" | "capture" | "win" {
  if (snapshot.gameState.result !== "ongoing") return "win";
  return snapshot.appliedMove?.captured ? "capture" : "move";
}

export default function App() {
  const clientRef = useRef<GameWorkerClient | null>(null);
  const snapshotRef = useRef<GameSnapshot | null>(null);
  const aiRevisionRef = useRef<number | null>(null);
  const testRunningRef = useRef(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const [phase, setPhase] = useState<AppPhase>("loading");
  const [progress, setProgress] = useState<ProgressPayload>(DEFAULT_PROGRESS);
  const [fatalError, setFatalError] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<GameSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [thinking, setThinking] = useState(false);
  const [searchProgress, setSearchProgress] = useState<SearchProgressPayload | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [targets, setTargets] = useState<LegalTarget[]>([]);
  const [selectedDiagnostics, setSelectedDiagnostics] = useState<LegalTargetsResult | null>(null);
  const [boardFlipped, setBoardFlipped] = useState(() => readBooleanPreference("jungle.boardFlipped", false));
  const [muted, setMuted] = useState(() => readBooleanPreference("jungle.muted", false));
  const [diagnostics, setDiagnostics] = useState(false);
  const [aiVsAi, setAiVsAi] = useState(false);
  const [aiPaused, setAiPaused] = useState(false);
  const [newGameOpen, setNewGameOpen] = useState(false);
  const [howToPlayOpen, setHowToPlayOpen] = useState(false);
  const [hasStarted, setHasStarted] = useState(false);
  const [lastUndoPlies, setLastUndoPlies] = useState(1);
  const [resultDismissedRevision, setResultDismissedRevision] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [recoverableError, setRecoverableError] = useState<string | null>(null);

  const updateSnapshot = useCallback((next: GameSnapshot, playAudio = false) => {
    snapshotRef.current = next;
    setSnapshot(next);
    setSelected(null);
    setTargets([]);
    setSelectedDiagnostics(null);
    setSearchProgress(null);
    if (playAudio && next.appliedMove) jungleAudio.play(cueForSnapshot(next));
  }, []);

  const initialize = useCallback(async (client: GameWorkerClient) => {
    setPhase("loading");
    setFatalError(null);
    setProgress(DEFAULT_PROGRESS);
    try {
      const assetBaseUrl = new URL(import.meta.env.BASE_URL, window.location.href).href;
      const initial = await client.initialize(assetBaseUrl);
      updateSnapshot(initial);
      setPhase("ready");
      setNewGameOpen(true);
    } catch (reason) {
      setFatalError(errorMessage(reason));
      setPhase("error");
    }
  }, [updateSnapshot]);

  useEffect(() => {
    const client = new GameWorkerClient({
      onProgress: setProgress,
      onThinking: (value) => {
        setThinking(value);
        if (!value) setSearchProgress(null);
      },
      onSearchProgress: setSearchProgress,
      onFatalError: (error) => {
        setFatalError(error.message);
        setPhase("error");
      },
    });
    clientRef.current = client;
    void initialize(client);
    return () => {
      client.destroy();
      clientRef.current = null;
    };
  }, [initialize]);

  useEffect(() => {
    jungleAudio.setMuted(muted);
    try {
      localStorage.setItem("jungle.muted", String(muted));
    } catch {
      // Local preferences are optional.
    }
  }, [muted]);

  useEffect(() => {
    try {
      localStorage.setItem("jungle.boardFlipped", String(boardFlipped));
    } catch {
      // Local preferences are optional.
    }
  }, [boardFlipped]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 3_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    if (!recoverableError) return;
    const timer = window.setTimeout(() => setRecoverableError(null), 6_000);
    return () => window.clearTimeout(timer);
  }, [recoverableError]);

  const retryInitialization = () => {
    const client = clientRef.current;
    if (!client) return;
    client.restart();
    void initialize(client);
  };

  const startGame = async (difficulty: Difficulty, humanSide: Side) => {
    const client = clientRef.current;
    if (!client) return;
    setBusy(true);
    setRecoverableError(null);
    try {
      const next = await client.request("newGame", { difficulty, humanSide });
      updateSnapshot(next);
      setAiVsAi(false);
      setAiPaused(false);
      setHasStarted(true);
      setNewGameOpen(false);
      setResultDismissedRevision(null);
      setLastUndoPlies(1);
    } catch (reason) {
      setRecoverableError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    const client = clientRef.current;
    if (
      !client ||
      !snapshot ||
      !hasStarted ||
      newGameOpen ||
      busy ||
      thinking ||
      testRunningRef.current ||
      aiPaused ||
      snapshot.gameState.result !== "ongoing"
    ) return;
    const aiTurn = aiVsAi || snapshot.gameState.side_to_move !== snapshot.humanSide;
    if (!aiTurn || aiRevisionRef.current === snapshot.revision) return;

    aiRevisionRef.current = snapshot.revision;
    setThinking(true);
    const nodeLimit = import.meta.env.VITE_ENABLE_TEST_HOOKS === "true"
      ? Number(new URLSearchParams(window.location.search).get("nodeLimit")) || undefined
      : undefined;
    const timeLimitMs = snapshot.difficulty === "hard" ? 2_000 : undefined;
    void client.request("aiMove", { difficulty: snapshot.difficulty, timeLimitMs, nodeLimit }).then((next) => {
      updateSnapshot(next, true);
    }).catch((reason: unknown) => {
      aiRevisionRef.current = null;
      if (!isSearchCancelled(reason)) setRecoverableError(errorMessage(reason));
    }).finally(() => setThinking(false));
  }, [aiPaused, aiVsAi, busy, hasStarted, newGameOpen, snapshot, thinking, updateSnapshot]);

  const handleSquare = async (canonicalIndex: number) => {
    const client = clientRef.current;
    if (!client || !snapshot || busy || thinking || aiVsAi || snapshot.gameState.result !== "ongoing") return;
    if (snapshot.gameState.side_to_move !== snapshot.humanSide) return;
    const target = targets.find((item) => item.destination === canonicalIndex);
    if (selected !== null && target) {
      setAiPaused(false);
      setBusy(true);
      try {
        const next = await client.request("applyMove", { origin: selected, destination: canonicalIndex });
        updateSnapshot(next, true);
      } catch (reason) {
        setRecoverableError(errorMessage(reason));
      } finally {
        setBusy(false);
      }
      return;
    }

    const piece = snapshot.gameState.board[canonicalIndex];
    if (selected === canonicalIndex) {
      setSelected(null);
      setTargets([]);
      setSelectedDiagnostics(null);
      return;
    }
    if (!piece || piece.side !== snapshot.humanSide) {
      setSelected(null);
      setTargets([]);
      setSelectedDiagnostics(null);
      return;
    }

    setBusy(true);
    try {
      const legal = await client.request("legalTargets", { origin: canonicalIndex });
      setSelected(canonicalIndex);
      setTargets(legal.targets);
      setSelectedDiagnostics(legal);
    } catch (reason) {
      setRecoverableError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const undo = async () => {
    const client = clientRef.current;
    if (!client || !snapshot) return;
    const requestedPlies = undoPlyCount(snapshot, aiVsAi);
    setAiVsAi(false);
    setAiPaused(false);
    setBusy(true);
    try {
      const next = await client.request("undo", { plies: requestedPlies });
      setLastUndoPlies(next.appliedPlies ?? requestedPlies);
      updateSnapshot(next);
      setResultDismissedRevision(null);
    } catch (reason) {
      setRecoverableError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const redo = async () => {
    const client = clientRef.current;
    if (!client) return;
    setBusy(true);
    setAiPaused(false);
    try {
      const next = await client.request("redo", { plies: Math.max(1, lastUndoPlies) });
      updateSnapshot(next);
      setResultDismissedRevision(null);
    } catch (reason) {
      setRecoverableError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const saveGame = async () => {
    const client = clientRef.current;
    if (!client) return;
    setBusy(true);
    try {
      const exported = await client.request("exportGame", {});
      downloadSave(createSave(exported, boardFlipped));
      setNotice("Game saved to your downloads.");
    } catch (reason) {
      setRecoverableError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const importSave = useCallback(async (save: JungleSave): Promise<GameSnapshot> => {
    const client = clientRef.current;
    if (!client) throw new Error("The game engine is not ready.");
    const next = await client.request("importGame", {
      gameState: save.gameState,
      difficulty: save.settings.difficulty,
      humanSide: save.settings.humanSide,
    });
    setBoardFlipped(save.settings.boardFlipped);
    setAiVsAi(false);
    setAiPaused(false);
    setHasStarted(true);
    setNewGameOpen(false);
    setResultDismissedRevision(null);
    updateSnapshot(next);
    return next;
  }, [updateSnapshot]);

  const onFileSelected = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    setBusy(true);
    try {
      const save = parseSave(await file.text());
      await importSave(save);
      setNotice("Saved game loaded.");
    } catch (reason) {
      setRecoverableError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (import.meta.env.VITE_ENABLE_TEST_HOOKS !== "true") return;
    window.__JUNGLE_TEST__ = {
      getSnapshot: () => snapshotRef.current,
      importSave,
      runVerifiedAiMove: async (nodeLimit = 100) => {
        const client = clientRef.current;
        const current = snapshotRef.current;
        if (!client || !current) throw new Error("Game engine is not ready.");
        const legalMoves: Array<{ origin: number; destination: number }> = [];
        testRunningRef.current = true;
        setAiVsAi(false);
        setAiPaused(false);
        try {
          for (let origin = 0; origin < current.gameState.board.length; origin += 1) {
            if (current.gameState.board[origin]?.side !== current.gameState.side_to_move) continue;
            const legal = await client.request("legalTargets", { origin });
            legalMoves.push(...legal.targets.map((target) => ({ origin, destination: target.destination })));
          }
          const next = await client.request("aiMove", {
            difficulty: current.difficulty,
            timeLimitMs: 10_000,
            nodeLimit,
          });
          updateSnapshot(next);
          return { beforeRevision: current.revision, legalMoves, snapshot: next };
        } finally {
          testRunningRef.current = false;
        }
      },
      runAiGame: async (nodeLimit = 60, maxPlies = 200) => {
        const client = clientRef.current;
        let current = snapshotRef.current;
        let maxTablebaseHits = 0;
        if (!client || !current) throw new Error("Game engine is not ready.");
        testRunningRef.current = true;
        setAiVsAi(false);
        setAiPaused(false);
        try {
          for (let ply = 0; ply < maxPlies && current.gameState.result === "ongoing"; ply += 1) {
            current = await client.request("aiMove", {
              difficulty: current.difficulty,
              timeLimitMs: 10_000,
              nodeLimit,
            });
            maxTablebaseHits = Math.max(maxTablebaseHits, current.aiStats?.tablebaseHits ?? 0);
            updateSnapshot(current);
            await new Promise((resolve) => window.setTimeout(resolve, 0));
          }
          return { snapshot: current, maxTablebaseHits };
        } finally {
          testRunningRef.current = false;
        }
      },
    };
    return () => {
      delete window.__JUNGLE_TEST__;
    };
  }, [importSave, updateSnapshot]);

  const toggleAiVsAi = () => {
    if (aiVsAi) {
      setAiVsAi(false);
      setAiPaused(true);
      const client = clientRef.current;
      if (client) {
        void client.cancelAiSearch().catch((reason: unknown) => {
          if (!isSearchCancelled(reason)) setRecoverableError(errorMessage(reason));
        });
      }
    } else {
      setAiPaused(false);
      setAiVsAi(true);
    }
  };

  if (phase === "loading") {
    return (
      <main className="loading-screen">
        <div className="loading-emblem" aria-hidden="true">虎</div>
        <span className="eyebrow">High-performance strategy</span>
        <h1>Entering the jungle</h1>
        <p aria-live="polite">{progress.message}</p>
        <div className="progress-track" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress.percent}>
          <span style={{ width: `${progress.percent}%` }} />
        </div>
        <small>First load includes the local engine and exact endgame tablebase.</small>
      </main>
    );
  }

  if (phase === "error" || !snapshot) {
    return (
      <main className="loading-screen loading-screen--error">
        <div className="loading-emblem" aria-hidden="true">!</div>
        <span className="eyebrow">Initialization stopped</span>
        <h1>The jungle could not open</h1>
        <p>{fatalError ?? "The game engine returned an invalid initial state."}</p>
        <button className="primary-button" type="button" onClick={retryInitialization}>Retry loading</button>
      </main>
    );
  }

  const boardDisabled =
    busy ||
    thinking ||
    aiVsAi ||
    snapshot.gameState.result !== "ongoing" ||
    snapshot.gameState.side_to_move !== snapshot.humanSide;
  const showResult = snapshot.gameState.result !== "ongoing" && resultDismissedRevision !== snapshot.revision;

  return (
    <div className="app-shell" onPointerDown={() => jungleAudio.unlock()}>
      <header className="app-header">
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true">鬥</div>
          <div>
            <span className="eyebrow">Dou Shou Qi</span>
            <h1>Jungle</h1>
          </div>
        </div>
        <div className="runtime-badge" title="All game logic runs locally in your browser">
          <span className="runtime-light" />
          {snapshot.runtime.engineName} {snapshot.runtime.engineVersion} · local Worker
        </div>
      </header>

      <main className="game-layout">
        <section className="board-column" aria-label="Game board">
          <Board
            snapshot={snapshot}
            flipped={boardFlipped}
            selected={selected}
            targets={targets}
            disabled={boardDisabled}
            onSquare={(index) => void handleSquare(index)}
          />
          <div className="board-meta">
            <span>Blue always moves first</span>
            <span>{boardFlipped ? "Flipped display" : "Canonical display"}</span>
            <span>{snapshot.difficulty[0].toUpperCase() + snapshot.difficulty.slice(1)} AI</span>
          </div>
        </section>

        <SidePanel
          snapshot={snapshot}
          thinking={thinking}
          searchProgress={searchProgress}
          busy={busy}
          aiVsAi={aiVsAi}
          aiPaused={aiPaused}
          diagnostics={diagnostics}
          selectedDiagnostics={selectedDiagnostics}
          muted={muted}
          onNewGame={() => setNewGameOpen(true)}
          onUndo={() => void undo()}
          onRedo={() => void redo()}
          onSave={() => void saveGame()}
          onLoad={() => fileInputRef.current?.click()}
          onFlip={() => setBoardFlipped((value) => !value)}
          onDiagnostics={() => setDiagnostics((value) => !value)}
          onAiVsAi={toggleAiVsAi}
          onMute={() => setMuted((value) => !value)}
          onHowToPlay={() => setHowToPlayOpen(true)}
        />
      </main>

      <footer className="app-footer">
        <span>App {packageJson.version} · Engine {snapshot.runtime.engineVersion} · WebAssembly · exact Jungle rules</span>
        <span>Search {snapshot.runtime.searchVersion} · local browser engine</span>
      </footer>

      <input ref={fileInputRef} className="visually-hidden" type="file" accept="application/json,.json" onChange={(event) => void onFileSelected(event)} />

      {recoverableError ? <div className="toast toast--error" role="alert">{recoverableError}</div> : null}
      {notice ? <div className="toast" role="status">{notice}</div> : null}

      {newGameOpen ? (
        <NewGameDialog
          currentDifficulty={snapshot.difficulty}
          canClose={hasStarted}
          onClose={() => setNewGameOpen(false)}
          onStart={startGame}
        />
      ) : null}
      {howToPlayOpen ? <HowToPlay onClose={() => {
        setHowToPlayOpen(false);
        try { localStorage.setItem("jungle.howToPlayDismissed", "true"); } catch { /* Optional. */ }
      }} /> : null}
      {showResult ? (
        <Modal title={`${snapshot.gameState.winner?.toUpperCase() ?? "Game"} wins`}>
          <div className="result-content">
            <div className={`result-orb result-orb--${snapshot.gameState.winner ?? "blue"}`} aria-hidden="true">★</div>
            <p>{snapshot.gameState.result_reason}</p>
            <div className="modal-actions">
              <button className="primary-button" type="button" onClick={() => setNewGameOpen(true)}>New Game</button>
              <button className="secondary-button" type="button" onClick={() => setResultDismissedRevision(snapshot.revision)}>Review board</button>
            </div>
          </div>
        </Modal>
      ) : null}
    </div>
  );
}
