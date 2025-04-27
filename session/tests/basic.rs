#![no_std]

use gtest::{Program, ProgramBuilder, System};
use session_io::*;

const USER: u64 = 3;
const SESSION_PROGRAM_ID: u64 = 1;
const WORDLE_PROGRAM_ID: u64 = 2;

#[test]
fn test_game_start() {
    let system = System::new();
    system.init_logger();

    let session_program: Program =
        ProgramBuilder::from_file("../target/wasm32-unknown-unknown/debug/session.opt.wasm")
            .with_id(SESSION_PROGRAM_ID)
            .build(&system);

    let wordle_program: Program =
        ProgramBuilder::from_file("../target/wasm32-unknown-unknown/debug/wordle.opt.wasm")
            .with_id(WORDLE_PROGRAM_ID)
            .build(&system);

    system.mint_to(USER, 1000000000000000);
    wordle_program.send_bytes(USER, []);
    system.run_next_block();

    session_program.send(USER, wordle_program.id());
    system.run_next_block();

    session_program.send(USER, Action::StartGame);
    system.run_next_block();

    session_program.send(
        USER,
        Action::CheckWord {
            word: "helle".into(),
        },
    );
    system.run_next_block();

    let state: State = session_program
        .read_state(())
        .expect("Failed to read state");
    assert_eq!(
        state.players.get(&USER.into()).unwrap().game_status,
        GameStatus::InProgress
    );
}

#[test]
fn test_game_win() {
    let system = System::new();
    system.init_logger();

    let session_program: Program =
        ProgramBuilder::from_file("../target/wasm32-unknown-unknown/debug/session.opt.wasm")
            .with_id(SESSION_PROGRAM_ID)
            .build(&system);

    let wordle_program: Program =
        ProgramBuilder::from_file("../target/wasm32-unknown-unknown/debug/wordle.opt.wasm")
            .with_id(WORDLE_PROGRAM_ID)
            .build(&system);

    system.mint_to(USER, 12345678987654321258);
    wordle_program.send_bytes(USER, []);
    system.run_next_block();
    session_program.send(USER, wordle_program.id());
    system.run_next_block();
    session_program.send(USER, Action::StartGame);
    system.run_next_block();

    session_program.send(
        USER,
        Action::CheckWord {
            word: "human".into(),
        },
    );
    system.run_next_block();

    session_program.send(
        USER,
        Action::CheckWord {
            word: "horse".into(),
        },
    );
    system.run_next_block();
    session_program.send(
        USER,
        Action::CheckWord {
            word: "house".into(),
        },
    );
    system.run_next_block();

    let state: State = session_program
        .read_state(())
        .expect("Failed to read state");
    assert_eq!(
        state.players.get(&USER.into()).unwrap().game_status,
        GameStatus::Completed(GameOverStatus::Win)
    );
}

#[test]
fn test_timeout() {
    let system = System::new();
    system.init_logger();

    let session_program: Program =
        ProgramBuilder::from_file("../target/wasm32-unknown-unknown/debug/session.opt.wasm")
            .with_id(SESSION_PROGRAM_ID)
            .build(&system);

    let wordle_program: Program =
        ProgramBuilder::from_file("../target/wasm32-unknown-unknown/debug/wordle.opt.wasm")
            .with_id(WORDLE_PROGRAM_ID)
            .build(&system);

    system.mint_to(USER, 12345678987654321258);
    wordle_program.send_bytes(USER, []);
    system.run_next_block();
    session_program.send(USER, wordle_program.id());
    system.run_next_block();
    session_program.send(USER, Action::StartGame);
    system.run_next_block();
    session_program.send(
        USER,
        Action::CheckWord {
            word: "hello".into(),
        },
    );
    system.run_next_block();
    system.run_to_block(210);

    session_program.send(
        USER,
        Action::CheckWord {
            word: "hello".into(),
        },
    );

    let state: State = session_program.read_state(()).unwrap();
    assert_eq!(
        state.players.get(&USER.into()).unwrap().game_status,
        GameStatus::Completed(GameOverStatus::Lose)
    );
}
