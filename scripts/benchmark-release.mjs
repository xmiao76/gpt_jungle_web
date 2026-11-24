import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync, copyFileSync } from "node:fs";
import { resolve } from "node:path";
const dir = "generated/strength",
  ref = "72ca198";
for (const sub of ["baseline/src", "src", "reports"])
  mkdirSync(dir + "/" + sub, { recursive: true });
const git = (...args) => execFileSync("git", args);
const files = git(
  "ls-tree",
  "-r",
  "--name-only",
  ref,
  "engine/crates/jungle-search/src",
)
  .toString()
  .trim()
  .split(/\r?\n/);
// Port only the draw enum and the accounting for simulated den-defense children.
// Both strategies therefore receive the same actual position ceiling.
for (const file of files) {
  let source = git("show", ref + ":" + file).toString();
  if (file.endsWith("/search.rs"))
    source = source
      .replace(
        "Outcome::Ongoing => 0,",
        "Outcome::Ongoing | Outcome::Draw { .. } => 0,",
      )
      .replace(
        "let mut completed_score = evaluator.evaluate(&board);",
        "let mut completed_score = 0;",
      )
      .replace(
        /legal_moves\s*\.iter\(\)\s*\.copied\(\)\s*\.filter\(\|movement\| defends_den\(board, \*movement\)\)\s*\.collect\(\)/,
        "{ let mut defenses=Vec::new(); for &m in legal_moves {self.enter_node(ply+1,true)?;if defends_den(board,m){defenses.push(m)}} defenses }",
      );
  writeFileSync(dir + "/baseline/src/" + file.split("/").at(-1), source);
}
writeFileSync(
  dir + "/baseline/Cargo.toml",
  '[package]\nname="baseline-search"\nversion="2.0.0"\nedition="2024"\n[dependencies]\njungle-core={path="../../../engine/crates/jungle-core"}\nserde={version="1",features=["derive"]}\nthiserror="2"\n',
);
writeFileSync(
  dir + "/Cargo.toml",
  '[package]\nname="strength"\nversion="2.0.0"\nedition="2024"\n[dependencies]\njungle-entry={path="../../engine/crates/jungle-entry"}\njungle-core={path="../../engine/crates/jungle-core"}\nbaseline-search={path="baseline"}\nserde_json="1"\nserde={version="1",features=["derive"]}\n[profile.release]\nopt-level=3\nlto=true\n',
);
writeFileSync(
  dir + "/src/main.rs",
  readFileSync("tools/strength.rs", "utf8").replace(
    "../engine/crates/jungle-cli/src/stats.rs",
    "stats.rs",
  ),
);
copyFileSync("engine/crates/jungle-cli/src/stats.rs", dir + "/src/stats.rs");
writeFileSync(dir + "/Cargo.lock", readFileSync("engine/Cargo.lock"));
execFileSync(
  "cargo",
  [
    "run",
    "--release",
    "--manifest-path",
    dir + "/Cargo.toml",
    "--",
    ...process.argv.slice(2),
  ],
  {
    stdio: "inherit",
    env: {
      ...process.env,
      CARGO_TARGET_DIR: resolve("engine/target/strength"),
    },
  },
);
