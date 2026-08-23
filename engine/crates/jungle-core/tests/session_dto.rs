use jungle_core::{
    CoordinatesDto, Difficulty, GameStateDto, JungleSaveV1, Move, Outcome, Piece, PieceKind,
    Position, ResultType, SaveSettingsV1, Session, SessionError, Side, WinReason,
};

fn piece(side: Side, kind: PieceKind) -> Piece {
    Piece::new(side, kind)
}

fn puzzle(placements: &[(u8, Piece)], side: Side) -> Session {
    Session::from_position(Position::from_piece_placements_checked(placements, side).unwrap())
}

#[test]
fn coordinates_are_strictly_checked_and_cannot_use_negative_indexing() {
    let mut session = Session::new();
    assert!(matches!(
        session.apply_coordinates(-1, 6),
        Err(SessionError::InvalidSquare(-1))
    ));
    assert!(matches!(
        session.apply_coordinates(63, 6),
        Err(SessionError::InvalidSquare(63))
    ));
    assert!(matches!(
        session.apply_coordinates(62, -1),
        Err(SessionError::InvalidSquare(-1))
    ));
    assert_eq!(session.position(), &Position::initial());

    let json = r#"{"origin":-1,"destination":6}"#;
    let coordinates: CoordinatesDto = serde_json::from_str(json).unwrap();
    assert!(matches!(
        session.apply_coordinates(coordinates.origin, coordinates.destination),
        Err(SessionError::InvalidSquare(-1))
    ));
}

#[test]
fn terminal_sessions_are_immutable_but_undo_redo_crosses_the_boundary() {
    let mut session = puzzle(
        &[
            (10, piece(Side::Blue, PieceKind::Cat)),
            (62, piece(Side::Red, PieceKind::Rat)),
        ],
        Side::Blue,
    );
    let winning = session.apply_coordinates(10, 3).unwrap();
    assert_eq!(
        session.outcome(),
        Outcome::Win {
            winner: Side::Blue,
            reason: WinReason::DenEntry
        }
    );
    assert!(matches!(
        session.apply_coordinates(62, 55),
        Err(SessionError::GameOver)
    ));
    assert_eq!(session.history(), &[winning]);

    assert!(session.undo());
    assert_eq!(session.outcome(), Outcome::Ongoing);
    assert_eq!(
        session.position().piece_at(10),
        Some(piece(Side::Blue, PieceKind::Cat))
    );
    assert!(session.redo());
    assert!(session.outcome().is_over());
    assert_eq!(
        session.position().piece_at(3),
        Some(piece(Side::Blue, PieceKind::Cat))
    );
}

#[test]
fn capture_all_and_no_legal_move_sessions_are_terminal() {
    let mut capture = puzzle(
        &[
            (14, piece(Side::Blue, PieceKind::Cat)),
            (15, piece(Side::Red, PieceKind::Rat)),
        ],
        Side::Blue,
    );
    capture.apply_coordinates(14, 15).unwrap();
    assert_eq!(
        capture.outcome(),
        Outcome::Win {
            winner: Side::Blue,
            reason: WinReason::CaptureAll
        }
    );
    assert!(matches!(
        capture.apply_coordinates(15, 14),
        Err(SessionError::GameOver)
    ));

    let mut no_moves = puzzle(
        &[
            (21, piece(Side::Blue, PieceKind::Cat)),
            (14, piece(Side::Red, PieceKind::Dog)),
            (28, piece(Side::Red, PieceKind::Wolf)),
        ],
        Side::Blue,
    );
    assert_eq!(
        no_moves.outcome(),
        Outcome::Win {
            winner: Side::Red,
            reason: WinReason::NoLegalMoves
        }
    );
    assert!(matches!(
        no_moves.apply_coordinates(21, 22),
        Err(SessionError::GameOver)
    ));
}

#[test]
fn game_state_and_save_v1_round_trip_strictly() {
    let mut session = Session::new();
    session.apply_coordinates(48, 41).unwrap();
    session.apply_coordinates(14, 21).unwrap();
    let state = session.game_state_dto();
    let json = serde_json::to_string(&state).unwrap();
    let loaded = Session::from_game_state_json(&json).unwrap();
    assert_eq!(loaded.position(), session.position());
    assert_eq!(loaded.history(), session.history());
    assert!(!loaded.can_undo());
    assert!(!loaded.can_redo());

    let settings = SaveSettingsV1 {
        difficulty: Difficulty::Hard,
        human_side: Side::Blue,
        board_flipped: false,
    };
    let save_json = session
        .export_save_v1_json("0.2.0", "2026-08-16T00:00:00.000Z", settings.clone())
        .unwrap();
    let (saved_session, save) = Session::import_save_v1_json(&save_json).unwrap();
    assert_eq!(save.schema, JungleSaveV1::SCHEMA);
    assert_eq!(save.version, 1);
    assert_eq!(save.settings, settings);
    assert_eq!(saved_session.position(), session.position());
    assert_eq!(saved_session.history(), session.history());
}

#[test]
fn historyless_strict_puzzles_remain_save_v1_compatible() {
    let session = puzzle(
        &[
            (21, piece(Side::Blue, PieceKind::Cat)),
            (14, piece(Side::Red, PieceKind::Dog)),
            (28, piece(Side::Red, PieceKind::Wolf)),
        ],
        Side::Blue,
    );
    let dto = session.game_state_dto();
    assert!(dto.move_history.is_empty());
    let loaded = Session::from_game_state_dto(dto).unwrap();
    assert_eq!(loaded.position(), session.position());
    assert!(loaded.outcome().is_over());
}

#[test]
fn moves_after_a_historyless_puzzle_serialize_from_the_current_position() {
    let mut session = puzzle(
        &[
            (28, piece(Side::Blue, PieceKind::Cat)),
            (34, piece(Side::Red, PieceKind::Dog)),
        ],
        Side::Blue,
    );
    session.apply_coordinates(28, 21).unwrap();
    assert_eq!(session.history().len(), 1, "the live UI log is retained");

    let dto = session.game_state_dto();
    assert!(
        dto.move_history.is_empty(),
        "save v1 cannot encode a nonstandard history origin"
    );
    let loaded = Session::from_game_state_dto(dto).unwrap();
    assert_eq!(loaded.position(), session.position());
    assert!(loaded.history().is_empty());
}

#[test]
fn malformed_boards_results_and_history_metadata_are_rejected() {
    let mut too_short = Session::new().game_state_dto();
    too_short.board.pop();
    assert!(matches!(
        Session::from_game_state_dto(too_short),
        Err(SessionError::InvalidState(_))
    ));

    let mut water_cat = Session::new().game_state_dto();
    water_cat.move_history.clear();
    water_cat.board[42] = None;
    water_cat.board[22] = Some(piece(Side::Blue, PieceKind::Elephant));
    assert!(matches!(
        Session::from_game_state_dto(water_cat),
        Err(SessionError::InvalidPosition(_))
    ));

    let mut duplicate = Session::new().game_state_dto();
    duplicate.move_history.clear();
    duplicate.board[43] = duplicate.board[42];
    assert!(matches!(
        Session::from_game_state_dto(duplicate),
        Err(SessionError::InvalidPosition(_))
    ));

    let mut wrong_result = Session::new().game_state_dto();
    wrong_result.result = ResultType::CaptureAll;
    wrong_result.winner = Some(Side::Blue);
    wrong_result.result_reason = "forged".to_owned();
    assert!(matches!(
        Session::from_game_state_dto(wrong_result),
        Err(SessionError::InvalidState(_))
    ));

    let mut one_move = Session::new();
    one_move.apply_coordinates(48, 41).unwrap();
    let mut tampered = one_move.game_state_dto();
    tampered.move_history[0] =
        Move::from_parts(48, 41, piece(Side::Blue, PieceKind::Cat), None, false).unwrap();
    assert!(matches!(
        Session::from_game_state_dto(tampered),
        Err(SessionError::InvalidHistory { .. })
    ));
}

fn play_reachable_den_win() -> Session {
    let mut session = Session::new();
    for (origin, destination) in [
        (48, 41),
        (18, 17),
        (41, 34),
        (0, 1),
        (34, 27),
        (1, 0),
        (27, 20),
        (0, 1),
        (20, 19),
        (1, 0),
        (19, 18),
        (0, 1),
        (18, 11),
        (1, 0),
        (11, 10),
        (0, 1),
        (10, 3),
    ] {
        session.apply_coordinates(origin, destination).unwrap();
    }
    assert_eq!(
        session.outcome(),
        Outcome::Win {
            winner: Side::Blue,
            reason: WinReason::DenEntry
        }
    );
    session
}

#[test]
fn imported_history_rejects_every_ply_after_the_first_terminal_result() {
    let terminal = play_reachable_den_win();
    let mut forged = terminal.game_state_dto();
    let red_lion = piece(Side::Red, PieceKind::Lion);
    let post_terminal = Move::from_parts(1, 0, red_lion, None, false).unwrap();
    forged.move_history.push(post_terminal);
    forged.board[1] = None;
    forged.board[0] = Some(red_lion);
    forged.side_to_move = Side::Blue;
    // Den entry remains structurally visible, so the declared result still
    // matches the final board. Replay must nevertheless reject ply 18.
    assert_eq!(forged.result, ResultType::DenEntry);
    assert!(matches!(
        Session::from_game_state_dto(forged),
        Err(SessionError::InvalidHistory { ply: 18, .. })
    ));
}

#[test]
fn move_json_rejects_negative_coordinates_bad_notes_and_unknown_fields() {
    let negative = r#"{
        "origin":-1,"destination":6,
        "piece":{"side":"blue","kind":"LION"},
        "captured":{"side":"red","kind":"TIGER"},
        "is_jump":false,"note":""
    }"#;
    assert!(serde_json::from_str::<Move>(negative).is_err());

    let bad_note = r#"{
        "origin":48,"destination":41,
        "piece":{"side":"blue","kind":"RAT"},
        "captured":null,"is_jump":false,"note":"jump"
    }"#;
    assert!(serde_json::from_str::<Move>(bad_note).is_err());

    let unknown = r#"{
        "origin":48,"destination":41,
        "piece":{"side":"blue","kind":"RAT"},
        "captured":null,"is_jump":false,"note":"","extra":true
    }"#;
    assert!(serde_json::from_str::<Move>(unknown).is_err());
}

#[test]
fn save_v1_rejects_unknown_schema_version_and_empty_metadata() {
    let settings = SaveSettingsV1 {
        difficulty: Difficulty::Medium,
        human_side: Side::Red,
        board_flipped: true,
    };
    let session = Session::new();
    let mut save = session.export_save_v1("2.0.0", "now", settings);
    save.schema = "other".to_owned();
    assert!(matches!(
        Session::from_save_v1(&save),
        Err(SessionError::InvalidSave(_))
    ));
    save.schema = JungleSaveV1::SCHEMA.to_owned();
    save.version = 2;
    assert!(matches!(
        Session::from_save_v1(&save),
        Err(SessionError::InvalidSave(_))
    ));
    save.version = 1;
    save.engine_version.clear();
    assert!(matches!(
        Session::from_save_v1(&save),
        Err(SessionError::InvalidSave(_))
    ));
}

#[test]
fn result_reason_and_wire_shape_match_existing_save_v1() {
    let state = Session::new().game_state_dto();
    let value = serde_json::to_value(&state).unwrap();
    assert_eq!(value["side_to_move"], "blue");
    assert_eq!(value["result"], "ongoing");
    assert_eq!(value["result_reason"], "");
    assert_eq!(value["board"].as_array().unwrap().len(), 63);
    assert_eq!(value["board"][48]["kind"], "RAT");

    let den = play_reachable_den_win().game_state_dto();
    assert_eq!(den.winner, Some(Side::Blue));
    assert_eq!(den.result, ResultType::DenEntry);
    assert_eq!(den.result_reason, "Blue entered the red den.");
}

#[test]
fn game_state_dto_is_cloneable_and_json_strict() {
    let dto: GameStateDto = Session::new().game_state_dto();
    assert_eq!(dto, dto.clone());
    let mut value = serde_json::to_value(&dto).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unknown".to_owned(), true.into());
    assert!(serde_json::from_value::<GameStateDto>(value).is_err());
}
