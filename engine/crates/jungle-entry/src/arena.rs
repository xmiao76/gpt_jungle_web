use crate::{Identity, Ply, Snapshot as State};
use jungle_core::{Piece, PieceKind, Side as Owner};
use std::cell::Cell;
wit_bindgen::generate!({path:"../../../wit",world:"competitor"});
use exports::jungle::arena::game as wire;
use wire::Guest;
struct Entrant;
thread_local! {static SETTINGS:Cell<(wire::Level,u32)>=const {Cell::new((wire::Level::Hard,0))};}
fn side(v: wire::Side) -> Owner {
    match v {
        wire::Side::Red => Owner::Red,
        wire::Side::Blue => Owner::Blue,
    }
}
fn state(v: wire::Snapshot) -> State {
    let board = v
        .board
        .into_iter()
        .map(|p| {
            p.map_or(0, |p| {
                Piece::new(
                    side(p.owner),
                    match p.kind {
                        wire::Animal::Rat => PieceKind::Rat,
                        wire::Animal::Cat => PieceKind::Cat,
                        wire::Animal::Dog => PieceKind::Dog,
                        wire::Animal::Wolf => PieceKind::Wolf,
                        wire::Animal::Leopard => PieceKind::Leopard,
                        wire::Animal::Tiger => PieceKind::Tiger,
                        wire::Animal::Lion => PieceKind::Lion,
                        wire::Animal::Elephant => PieceKind::Elephant,
                    },
                )
                .code()
            })
        })
        .collect();
    State {
        rev: v.rev,
        board,
        first_side: side(v.first_side),
        to_play: side(v.to_play),
        history: v
            .history
            .into_iter()
            .map(|m| Ply {
                src: m.src,
                dst: m.dst,
            })
            .collect(),
        no_capture_plies: v.no_capture_plies,
        status: match v.status {
            wire::Phase::Playing => "playing",
            wire::Phase::Drawn => "drawn",
            wire::Phase::Won => "won",
            wire::Phase::Forfeited => "forfeited",
        }
        .into(),
        reason: v.reason,
        winner: v.winner.map(side),
    }
}
impl Guest for Entrant {
    fn info() -> wire::Metadata {
        let id = Identity::load();
        wire::Metadata {
            title: if cfg!(feature = "fixture") {
                "First legal fixture".into()
            } else {
                id.title()
            },
            game_version: id.game_version,
            model: id.model,
            code_agent: id.code_agent,
            model_platform: id.model_platform,
            github_url: id.github_url,
            runtime: id.runtime,
            engine_version: jungle_core::ENGINE_VERSION.into(),
            rules_id: jungle_core::RULESET_ID.into(),
            rules_digest: jungle_core::RULES_HASH_HEX.into(),
        }
    }
    fn init(config: wire::Settings) {
        SETTINGS.set((config.difficulty, config.seed));
    }
    fn choose_move(snapshot: wire::Snapshot, legal: Vec<wire::PlyMove>, node_budget: u32) -> u32 {
        if cfg!(feature = "fixture") {
            return 0;
        }
        let (level, seed) = SETTINGS.get();
        let legal = legal
            .into_iter()
            .map(|m| Ply {
                src: m.src,
                dst: m.dst,
            })
            .collect::<Vec<_>>();
        crate::fixed(
            &state(snapshot),
            &legal,
            match level {
                wire::Level::Easy => "easy",
                wire::Level::Medium => "medium",
                wire::Level::Hard => "hard",
            },
            seed,
            node_budget,
        )
        .map_or(0, |r| r.index as u32)
    }
    fn render(snapshot: wire::Snapshot, flipped: bool, progress: u16) {
        let rgba = crate::paint::frame(
            &state(snapshot),
            flipped,
            progress,
            cfg!(feature = "fixture"),
        );
        jungle::arena::display::present(448, 576, &rgba);
    }
}
export!(Entrant);
