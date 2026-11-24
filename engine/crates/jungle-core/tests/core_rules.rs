use jungle_core::{
    BLUE_DEN, BOARD_SIZE, MoveError, Outcome, Piece, PieceKind, Position, PositionError, RED_DEN,
    RULES_HASH, RULES_MANIFEST_JSON, Side, WinReason,
};
use sha2::{Digest, Sha256};

fn piece(side: Side, kind: PieceKind) -> Piece {
    Piece::new(side, kind)
}

fn position(placements: &[(u8, Piece)], side: Side) -> Position {
    Position::from_piece_placements_checked(placements, side).unwrap()
}

#[test]
fn initial_setup_and_rules_fingerprint_are_exact() {
    let state = Position::initial();
    assert_eq!(state.side_to_move(), Side::Red);
    assert_eq!(state.piece_count(Side::Red), 8);
    assert_eq!(state.piece_count(Side::Blue), 8);
    assert_eq!(
        state.piece_at(42),
        Some(piece(Side::Red, PieceKind::Elephant))
    );
    assert_eq!(state.piece_at(44), Some(piece(Side::Red, PieceKind::Wolf)));
    assert_eq!(
        state.piece_at(46),
        Some(piece(Side::Red, PieceKind::Leopard))
    );
    assert_eq!(state.piece_at(48), Some(piece(Side::Red, PieceKind::Rat)));
    assert_eq!(state.piece_at(50), Some(piece(Side::Red, PieceKind::Cat)));
    assert_eq!(state.piece_at(54), Some(piece(Side::Red, PieceKind::Dog)));
    assert_eq!(state.piece_at(56), Some(piece(Side::Red, PieceKind::Tiger)));
    assert_eq!(state.piece_at(62), Some(piece(Side::Red, PieceKind::Lion)));
    assert_eq!(state.piece_at(0), Some(piece(Side::Blue, PieceKind::Lion)));
    assert_eq!(state.piece_at(6), Some(piece(Side::Blue, PieceKind::Tiger)));
    assert_eq!(state.piece_at(8), Some(piece(Side::Blue, PieceKind::Dog)));
    assert_eq!(state.piece_at(12), Some(piece(Side::Blue, PieceKind::Cat)));
    assert_eq!(state.piece_at(14), Some(piece(Side::Blue, PieceKind::Rat)));
    assert_eq!(
        state.piece_at(16),
        Some(piece(Side::Blue, PieceKind::Leopard))
    );
    assert_eq!(state.piece_at(18), Some(piece(Side::Blue, PieceKind::Wolf)));
    assert_eq!(
        state.piece_at(20),
        Some(piece(Side::Blue, PieceKind::Elephant))
    );
    assert_eq!(state.legal_moves().len(), 24);
    assert_eq!(state.hash(), state.recompute_zobrist());

    assert_eq!(
        Sha256::digest(RULES_MANIFEST_JSON.as_bytes()).as_slice(),
        RULES_HASH
    );
}

#[test]
fn exhaustive_land_capture_matrix_preserves_custom_ranks() {
    for attacker_kind in PieceKind::ALL {
        for defender_kind in PieceKind::ALL {
            let state = position(
                &[
                    (27, piece(Side::Red, attacker_kind)),
                    (20, piece(Side::Blue, defender_kind)),
                ],
                Side::Red,
            );
            let expected =
                if attacker_kind == PieceKind::Rat && defender_kind == PieceKind::Elephant {
                    true
                } else if attacker_kind == PieceKind::Elephant && defender_kind == PieceKind::Rat {
                    false
                } else {
                    attacker_kind.rank() >= defender_kind.rank()
                };
            assert_eq!(
                state.can_capture(27, 20),
                expected,
                "{attacker_kind:?} attacking {defender_kind:?}"
            );
        }
    }
    assert_eq!(PieceKind::Dog.rank(), 3);
    assert_eq!(PieceKind::Wolf.rank(), 4);
}

#[test]
fn water_shore_rat_elephant_and_trap_capture_rules_are_exact() {
    let water_rats = position(
        &[
            (22, piece(Side::Red, PieceKind::Rat)),
            (23, piece(Side::Blue, PieceKind::Rat)),
        ],
        Side::Red,
    );
    assert!(water_rats.can_capture(22, 23));

    let water_to_land = position(
        &[
            (22, piece(Side::Red, PieceKind::Rat)),
            (15, piece(Side::Blue, PieceKind::Elephant)),
        ],
        Side::Red,
    );
    assert!(!water_to_land.can_capture(22, 15));

    let land_to_water = position(
        &[
            (21, piece(Side::Red, PieceKind::Rat)),
            (22, piece(Side::Blue, PieceKind::Rat)),
        ],
        Side::Red,
    );
    assert!(!land_to_water.can_capture(21, 22));

    let trapped_elephant = position(
        &[
            (57, piece(Side::Red, PieceKind::Cat)),
            (58, piece(Side::Blue, PieceKind::Elephant)),
        ],
        Side::Red,
    );
    assert_eq!(trapped_elephant.effective_rank(58), 0);
    assert!(trapped_elephant.can_capture(57, 58));

    let trapped_rat = position(
        &[
            (57, piece(Side::Red, PieceKind::Elephant)),
            (58, piece(Side::Blue, PieceKind::Rat)),
        ],
        Side::Red,
    );
    assert!(trapped_rat.can_capture(57, 58));

    let rat_in_own_trap = position(
        &[
            (1, piece(Side::Red, PieceKind::Elephant)),
            (2, piece(Side::Blue, PieceKind::Rat)),
        ],
        Side::Red,
    );
    assert_eq!(rat_in_own_trap.effective_rank(2), 1);
    assert!(!rat_in_own_trap.can_capture(1, 2));

    // Trap weakening precedes the rat/elephant exception.
    let trapped_attacking_rat = position(
        &[
            (2, piece(Side::Red, PieceKind::Rat)),
            (1, piece(Side::Blue, PieceKind::Elephant)),
        ],
        Side::Red,
    );
    assert_eq!(trapped_attacking_rat.effective_rank(2), 0);
    assert!(!trapped_attacking_rat.can_capture(2, 1));
}

#[test]
fn lion_and_tiger_jump_spans_and_rat_blockers_are_exact() {
    let lion = position(
        &[
            (15, piece(Side::Red, PieceKind::Lion)),
            (62, piece(Side::Blue, PieceKind::Cat)),
        ],
        Side::Red,
    );
    let long = lion.find_legal_move(15, 43).unwrap();
    assert!(long.is_jump());

    let tiger = position(
        &[
            (15, piece(Side::Red, PieceKind::Tiger)),
            (62, piece(Side::Blue, PieceKind::Cat)),
        ],
        Side::Red,
    );
    assert_eq!(tiger.find_legal_move(15, 43), None);

    for kind in [PieceKind::Lion, PieceKind::Tiger] {
        let short = position(
            &[
                (21, piece(Side::Red, kind)),
                (62, piece(Side::Blue, PieceKind::Cat)),
            ],
            Side::Red,
        );
        assert!(short.find_legal_move(21, 24).unwrap().is_jump());
    }

    for blocker_side in [Side::Red, Side::Blue] {
        let blocked = position(
            &[
                (15, piece(Side::Red, PieceKind::Lion)),
                (29, piece(blocker_side, PieceKind::Rat)),
                (62, piece(Side::Blue, PieceKind::Cat)),
            ],
            Side::Red,
        );
        assert_eq!(blocked.find_legal_move(15, 43), None);
    }

    let jump_capture = position(
        &[
            (21, piece(Side::Red, PieceKind::Lion)),
            (24, piece(Side::Blue, PieceKind::Tiger)),
        ],
        Side::Red,
    );
    let movement = jump_capture.find_legal_move(21, 24).unwrap();
    assert!(movement.is_jump());
    assert_eq!(
        movement.captured(),
        Some(piece(Side::Blue, PieceKind::Tiger))
    );
}

#[test]
fn own_den_is_never_a_legal_destination() {
    let state = position(
        &[
            (52, piece(Side::Red, PieceKind::Cat)),
            (62, piece(Side::Blue, PieceKind::Rat)),
        ],
        Side::Red,
    );
    assert_eq!(state.find_legal_move(52, RED_DEN), None);
}

#[test]
fn outcomes_cover_den_capture_all_and_a_valid_no_move_position() {
    let den = position(
        &[
            (BLUE_DEN, piece(Side::Red, PieceKind::Cat)),
            (62, piece(Side::Blue, PieceKind::Rat)),
        ],
        Side::Blue,
    );
    assert_eq!(
        den.outcome(),
        Outcome::Win {
            winner: Side::Red,
            reason: WinReason::DenEntry
        }
    );
    assert!(den.legal_moves().is_empty());

    let capture_all = position(&[(21, piece(Side::Red, PieceKind::Cat))], Side::Blue);
    assert_eq!(
        capture_all.outcome(),
        Outcome::Win {
            winner: Side::Red,
            reason: WinReason::CaptureAll
        }
    );

    let no_moves = position(
        &[
            (21, piece(Side::Red, PieceKind::Cat)),
            (14, piece(Side::Blue, PieceKind::Dog)),
            (28, piece(Side::Blue, PieceKind::Wolf)),
        ],
        Side::Red,
    );
    assert_eq!(
        no_moves.outcome(),
        Outcome::Win {
            winner: Side::Blue,
            reason: WinReason::NoLegalMoves
        }
    );
    assert!(no_moves.legal_moves().is_empty());
}

#[test]
fn strict_position_construction_rejects_impossible_states() {
    assert_eq!(
        Position::from_piece_placements_checked(&[], Side::Red),
        Err(PositionError::EmptyPosition)
    );
    assert!(matches!(
        Position::from_piece_placements_checked(
            &[
                (22, piece(Side::Red, PieceKind::Cat)),
                (62, piece(Side::Blue, PieceKind::Rat)),
            ],
            Side::Red,
        ),
        Err(PositionError::NonRatInWater(22))
    ));
    assert!(matches!(
        Position::from_piece_placements_checked(
            &[
                (RED_DEN, piece(Side::Red, PieceKind::Cat)),
                (62, piece(Side::Blue, PieceKind::Rat)),
            ],
            Side::Red,
        ),
        Err(PositionError::OwnDenOccupied(Side::Red))
    ));
    assert!(matches!(
        Position::from_piece_placements_checked(
            &[
                (41, piece(Side::Red, PieceKind::Cat)),
                (48, piece(Side::Red, PieceKind::Cat)),
                (62, piece(Side::Blue, PieceKind::Rat)),
            ],
            Side::Red,
        ),
        Err(PositionError::DuplicatePiece { .. })
    ));
    assert!(matches!(
        Position::from_piece_placements_checked(
            &[
                (41, piece(Side::Red, PieceKind::Cat)),
                (41, piece(Side::Blue, PieceKind::Dog)),
            ],
            Side::Red,
        ),
        Err(PositionError::SquareOccupied(41))
    ));
    assert!(matches!(
        Position::from_piece_placements_checked(
            &[
                (BLUE_DEN, piece(Side::Red, PieceKind::Cat)),
                (RED_DEN, piece(Side::Blue, PieceKind::Dog)),
            ],
            Side::Red,
        ),
        Err(PositionError::BothDensWon)
    ));
}

#[test]
fn checked_position_application_rejects_terminal_moves() {
    let mut den = position(
        &[
            (BLUE_DEN, piece(Side::Red, PieceKind::Cat)),
            (62, piece(Side::Blue, PieceKind::Rat)),
        ],
        Side::Blue,
    );
    let forged =
        jungle_core::Move::from_parts(62, 55, piece(Side::Blue, PieceKind::Rat), None, false)
            .unwrap();
    assert_eq!(den.try_make_move(forged), Err(MoveError::GameOver));
}

fn perft(position: &mut Position, depth: u8) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = position.legal_moves();
    let mut nodes = 0;
    for &movement in moves.as_slice() {
        let undo = position.make_move_unchecked(movement);
        nodes += perft(position, depth - 1);
        position.unmake_move(movement, undo);
    }
    nodes
}

#[test]
fn initial_position_perft_matches_the_authoritative_python_engine() {
    let mut state = Position::initial();
    assert_eq!(perft(&mut state, 1), 24);
    assert_eq!(perft(&mut state, 2), 576);
    assert_eq!(perft(&mut state, 3), 12_240);
    assert_eq!(perft(&mut state, 4), 260_099);
    assert!(state.is_consistent());
}

#[test]
fn tactical_perft_fixtures_lock_jump_trap_and_water_semantics() {
    let fixtures: [(Position, [u64; 4]); 4] = [
        (
            position(
                &[
                    (15, piece(Side::Red, PieceKind::Lion)),
                    (62, piece(Side::Blue, PieceKind::Cat)),
                ],
                Side::Red,
            ),
            [4, 8, 30, 90],
        ),
        (
            position(
                &[
                    (15, piece(Side::Red, PieceKind::Tiger)),
                    (62, piece(Side::Blue, PieceKind::Cat)),
                ],
                Side::Red,
            ),
            [3, 6, 20, 60],
        ),
        (
            position(
                &[
                    (57, piece(Side::Red, PieceKind::Elephant)),
                    (58, piece(Side::Blue, PieceKind::Rat)),
                    (0, piece(Side::Blue, PieceKind::Cat)),
                ],
                Side::Red,
            ),
            [3, 12, 25, 128],
        ),
        (
            position(
                &[
                    (22, piece(Side::Red, PieceKind::Rat)),
                    (23, piece(Side::Blue, PieceKind::Rat)),
                    (62, piece(Side::Blue, PieceKind::Cat)),
                ],
                Side::Red,
            ),
            [4, 20, 72, 416],
        ),
    ];
    for (mut state, expected) in fixtures {
        for (depth, nodes) in (1_u8..=4).zip(expected) {
            assert_eq!(perft(&mut state, depth), nodes);
        }
    }
}

#[test]
fn make_unmake_and_incremental_hash_restore_exactly() {
    let initial = Position::initial();
    let mut state = initial.clone();
    let mut line = Vec::new();
    let mut selector = 0x0000_c0de_c0de_u64;
    for _ in 0..120 {
        let moves = state.legal_moves();
        if moves.is_empty() {
            break;
        }
        selector = selector
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let movement = moves.as_slice()[selector as usize % moves.len()];
        let undo = state.make_move_unchecked(movement);
        assert_eq!(state.hash(), state.recompute_zobrist());
        assert!(state.is_consistent());
        line.push((movement, undo));
    }
    assert!(line.len() >= 40);
    while let Some((movement, undo)) = line.pop() {
        state.unmake_move(movement, undo);
        assert_eq!(state.hash(), state.recompute_zobrist());
        assert!(state.is_consistent());
    }
    assert_eq!(state, initial);
}

#[test]
fn packed_moves_round_trip_and_reject_malformed_values() {
    let state = Position::initial();
    for movement in state.legal_moves().as_slice() {
        assert_eq!(
            jungle_core::Move::from_packed(movement.packed()),
            Some(*movement)
        );
    }
    assert_eq!(jungle_core::Move::from_packed(0), None);
    assert_eq!(jungle_core::Move::from_packed(u32::MAX), None);
    assert_eq!(BOARD_SIZE, 63);
}
