use std::env;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;

use crate::arena::{ArenaConfig, CorpusSource, EngineCommandSpec, run_arena};
use crate::protocol::LimitSpec;
use crate::server::{NativeBackend, serve_jsonl};

const HELP: &str = r#"jungle-cli

USAGE:
  jungle-cli [serve]
  jungle-cli arena --candidate <program|self> --champion <program|self> [options]

ARENA OPTIONS:
  --candidate-arg <arg>       Repeatable candidate argument
  --champion-arg <arg>        Repeatable champion argument
  --corpus <path>             Versioned opening corpus (default: embedded smoke corpus)
  --smoke                     Use the embedded initial-position smoke corpus
  --nodes <count>             Fixed-node limit (default: 2000)
  --time-ms <milliseconds>    Exact wall-clock limit; mutually exclusive with --nodes
  --max-depth <depth>         Search depth cap
  --max-pairs <count>         Deterministic corpus prefix
  --bootstrap-samples <count> Paired bootstrap samples (default: 100000)
  --bootstrap-seed <seed>     Bootstrap/game seed
  --protocol-timeout-ms <ms>  Non-time-search response timeout
  --deadline-grace-ms <ms>    Process transport grace after exact search deadline
  --require-promotion         Exit unsuccessfully unless promotion criteria pass
  --pretty                    Pretty-print arena JSON
"#;

#[derive(Debug)]
pub enum Mode {
    Serve,
    Arena {
        config: Box<ArenaConfig>,
        require_promotion: bool,
        pretty: bool,
    },
    Help,
}

pub fn run_from_env() -> Result<(), Box<dyn std::error::Error>> {
    match parse_args(env::args().skip(1))? {
        Mode::Serve => {
            let input = io::stdin();
            let output = io::stdout();
            let mut backend = NativeBackend::new();
            serve_jsonl(
                BufReader::new(input.lock()),
                BufWriter::new(output.lock()),
                &mut backend,
            )?;
            Ok(())
        }
        Mode::Arena {
            config,
            require_promotion,
            pretty,
        } => {
            let summary = run_arena(&config)?;
            if pretty {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("{}", serde_json::to_string(&summary)?);
            }
            if require_promotion && !summary.promotion.ready {
                return Err("candidate did not satisfy champion promotion criteria".into());
            }
            Ok(())
        }
        Mode::Help => {
            print!("{HELP}");
            Ok(())
        }
    }
}

pub fn parse_args(
    arguments: impl IntoIterator<Item = String>,
) -> Result<Mode, Box<dyn std::error::Error>> {
    let mut arguments = arguments.into_iter();
    let Some(mode) = arguments.next() else {
        return Ok(Mode::Serve);
    };
    match mode.as_str() {
        "serve" => {
            if arguments.next().is_some() {
                return Err("serve accepts no arguments".into());
            }
            Ok(Mode::Serve)
        }
        "-h" | "--help" | "help" => Ok(Mode::Help),
        "arena" => parse_arena(arguments.collect()),
        _ => Err(format!("unknown mode {mode:?}; use --help").into()),
    }
}

fn parse_arena(arguments: Vec<String>) -> Result<Mode, Box<dyn std::error::Error>> {
    let mut candidate_program = None;
    let mut champion_program = None;
    let mut candidate_arguments = Vec::new();
    let mut champion_arguments = Vec::new();
    let mut corpus = CorpusSource::Smoke;
    let mut nodes = Some(2_000_u64);
    let mut time_ms = None;
    let mut explicit_limit = None;
    let mut max_depth = None;
    let mut maximum_pairs = None;
    let mut bootstrap_samples = crate::stats::DEFAULT_BOOTSTRAP_SAMPLES;
    let mut bootstrap_seed = crate::stats::DEFAULT_BOOTSTRAP_SEED;
    let mut protocol_timeout_ms = crate::arena::DEFAULT_PROTOCOL_TIMEOUT_MS;
    let mut deadline_grace_ms = crate::arena::DEFAULT_DEADLINE_GRACE_MS;
    let mut require_promotion = false;
    let mut pretty = false;

    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        match flag {
            "--candidate" => candidate_program = Some(value(&arguments, &mut index, flag)?),
            "--champion" => champion_program = Some(value(&arguments, &mut index, flag)?),
            "--candidate-arg" => candidate_arguments.push(value(&arguments, &mut index, flag)?),
            "--champion-arg" => champion_arguments.push(value(&arguments, &mut index, flag)?),
            "--corpus" => {
                corpus = CorpusSource::File {
                    path: PathBuf::from(value(&arguments, &mut index, flag)?),
                };
            }
            "--smoke" => corpus = CorpusSource::Smoke,
            "--nodes" => {
                if explicit_limit == Some("time") {
                    return Err("--nodes and --time-ms are mutually exclusive".into());
                }
                explicit_limit = Some("nodes");
                nodes = Some(parse_value(&arguments, &mut index, flag)?);
                time_ms = None;
            }
            "--time-ms" => {
                if explicit_limit == Some("nodes") {
                    return Err("--nodes and --time-ms are mutually exclusive".into());
                }
                explicit_limit = Some("time");
                time_ms = Some(parse_value(&arguments, &mut index, flag)?);
                nodes = None;
            }
            "--max-depth" => max_depth = Some(parse_value(&arguments, &mut index, flag)?),
            "--max-pairs" => maximum_pairs = Some(parse_value(&arguments, &mut index, flag)?),
            "--bootstrap-samples" => {
                bootstrap_samples = parse_value(&arguments, &mut index, flag)?;
            }
            "--bootstrap-seed" => {
                bootstrap_seed = parse_value(&arguments, &mut index, flag)?;
            }
            "--protocol-timeout-ms" => {
                protocol_timeout_ms = parse_value(&arguments, &mut index, flag)?;
            }
            "--deadline-grace-ms" => {
                deadline_grace_ms = parse_value(&arguments, &mut index, flag)?;
            }
            "--require-promotion" => require_promotion = true,
            "--pretty" => pretty = true,
            "-h" | "--help" => return Ok(Mode::Help),
            _ => return Err(format!("unknown arena option {flag:?}").into()),
        }
        index += 1;
    }

    let candidate = command_spec(
        candidate_program.ok_or("arena requires --candidate")?,
        candidate_arguments,
    )?;
    let champion = command_spec(
        champion_program.ok_or("arena requires --champion")?,
        champion_arguments,
    )?;
    let limits = match (nodes, time_ms) {
        (Some(nodes), None) => LimitSpec::FixedNodes { nodes, max_depth },
        (None, Some(time_ms)) => LimitSpec::ExactTime { time_ms, max_depth },
        _ => return Err("select exactly one of --nodes and --time-ms".into()),
    };
    limits.to_search_limits().map_err(|error| error.message)?;
    if maximum_pairs == Some(0) {
        return Err("--max-pairs must be greater than zero".into());
    }
    if bootstrap_samples == 0 || protocol_timeout_ms == 0 {
        return Err("bootstrap samples and protocol timeout must be greater than zero".into());
    }
    if bootstrap_samples > crate::stats::MAX_BOOTSTRAP_SAMPLES {
        return Err(format!(
            "--bootstrap-samples must not exceed {}",
            crate::stats::MAX_BOOTSTRAP_SAMPLES
        )
        .into());
    }

    let mut config = ArenaConfig::with_defaults(candidate, champion, corpus, limits);
    config.maximum_pairs = maximum_pairs;
    config.bootstrap_samples = bootstrap_samples;
    config.bootstrap_seed = bootstrap_seed;
    config.protocol_timeout_ms = protocol_timeout_ms;
    config.deadline_grace_ms = deadline_grace_ms;
    Ok(Mode::Arena {
        config: Box::new(config),
        require_promotion,
        pretty,
    })
}

fn value(
    arguments: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    *index += 1;
    arguments
        .get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn parse_value<T: std::str::FromStr>(
    arguments: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<T, Box<dyn std::error::Error>>
where
    T::Err: std::fmt::Display,
{
    let raw = value(arguments, index, flag)?;
    raw.parse()
        .map_err(|error| format!("invalid {flag} value {raw:?}: {error}").into())
}

fn command_spec(
    program: String,
    mut arguments: Vec<String>,
) -> Result<EngineCommandSpec, Box<dyn std::error::Error>> {
    if program == "self" {
        let executable = env::current_exe()?;
        arguments.insert(0, "serve".to_owned());
        Ok(EngineCommandSpec {
            program: executable,
            arguments,
        })
    } else {
        Ok(EngineCommandSpec {
            program: PathBuf::from(program),
            arguments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mode_is_jsonl_serve() {
        assert!(matches!(
            parse_args(Vec::<String>::new()).expect("args"),
            Mode::Serve
        ));
    }

    #[test]
    fn arena_defaults_to_smoke_and_fixed_nodes() {
        let mode = parse_args([
            "arena".to_owned(),
            "--candidate".to_owned(),
            "candidate".to_owned(),
            "--champion".to_owned(),
            "champion".to_owned(),
        ])
        .expect("args");
        let Mode::Arena { config, .. } = mode else {
            panic!("arena mode");
        };
        assert!(matches!(config.corpus, CorpusSource::Smoke));
        assert_eq!(
            config.limits,
            LimitSpec::FixedNodes {
                nodes: 2_000,
                max_depth: None,
            }
        );
    }

    #[test]
    fn exact_time_replaces_default_node_limit() {
        let mode = parse_args([
            "arena".to_owned(),
            "--candidate".to_owned(),
            "candidate".to_owned(),
            "--champion".to_owned(),
            "champion".to_owned(),
            "--time-ms".to_owned(),
            "1800".to_owned(),
        ])
        .expect("args");
        let Mode::Arena { config, .. } = mode else {
            panic!("arena mode");
        };
        assert!(matches!(
            config.limits,
            LimitSpec::ExactTime { time_ms: 1800, .. }
        ));
    }
}
