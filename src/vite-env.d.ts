/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_ENABLE_TEST_HOOKS?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

interface Window {
  __JUNGLE_TEST__?: {
    getSnapshot: () => import("./game/types").GameSnapshot | null;
    importSave: (save: import("./game/types").JungleSave) => Promise<import("./game/types").GameSnapshot>;
    runVerifiedAiMove: (nodeLimit?: number) => Promise<{
      beforeRevision: number;
      legalMoves: Array<{ origin: number; destination: number }>;
      snapshot: import("./game/types").GameSnapshot;
    }>;
    runAiGame: (nodeLimit?: number, maxPlies?: number) => Promise<{
      snapshot: import("./game/types").GameSnapshot;
      maxTablebaseHits: number;
    }>;
  };
}
