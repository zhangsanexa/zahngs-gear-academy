#![no_std]
#![allow(warnings)]
use gstd::{collections::BTreeMap, debug, exec, msg, prelude::*, ActorId, MessageId};
use session_io::*;
use wordle_io::{Action as WordleAction, Event as WordleEvent};

macro_rules! reply {
    ($payload:expr) => {{
        use gstd::msg;
        msg::reply($payload, 0).expect("Error in sending reply");
    }};
}

macro_rules! create_inner_state {
    ($ident:ident, $type:ty) => {
        static mut $ident: Option<$type> = None;

        unsafe fn init_inner_state(val: $type) {
            $ident = Some(val);
        }

        fn get_inner_state_mut() -> &'static mut $type {
            unsafe { $ident.as_mut().expect("State is not initialized") }
        }

        fn get_inner_state() -> $type {
            unsafe { $ident.take().expect("State is not initialized") }
        }
    };
}

create_inner_state!(SESSION, Session);

struct Session {
    pub target_program_id: ActorId,
    pub players: BTreeMap<ActorId, PlayerInfo>,
}

impl Session {
    pub fn new(target_program_id: ActorId) -> Self {
        Self {
            target_program_id,
            players: BTreeMap::new(),
        }
    }

    pub fn start_game(&mut self, user: ActorId) {
        debug!("Starting game for user: {:?}", user);

        if let Some(player) = self.players.get_mut(&user) {
            debug!("Found existing player info");
            // ensure the game is not progressing
            assert!(
                !player.is_playing(),
                "{}",
                "A game is in progress for this user"
            );

            if player.game_status == GameStatus::Started {
                debug!("Game already started, setting to in progress");
                return Self::set_status_and_reply(
                    player,
                    GameStatus::InProgress,
                    Event::GameStarted,
                );
            }
        }

        debug!("Sending StartGame message to Wordle program");
        // Send `StartGame` message to Wordle program
        let sent_msg_id = msg::send(self.target_program_id, WordleAction::StartGame { user }, 0)
            .expect("Error in sending message");
        let original_msg_id = msg::id();
        debug!("Message sent with ID: {:?}", sent_msg_id);

        self.players
            .insert(user, PlayerInfo::new(sent_msg_id, original_msg_id));

        debug!("Sending delayed CheckGameStatus message");
        // Send a delayed message with `CheckGameStatus` action to monitor game's progress
        msg::send_delayed(
            exec::program_id(),
            Action::CheckGameStatus {
                user,
                init_id: original_msg_id,
            },
            0,
            200, // DELAY_CHECK_STATUS_DURATION
        )
        .expect("Error in sending delayed message");

        debug!("Waiting for response");
        // Wait for the response
        exec::wait();
    }

    pub fn check_word(&mut self, user: ActorId, word: String) {
        debug!("Checking word '{}' for user: {:?}", word, user);

        let player = self
            .players
            .get_mut(&user)
            .expect("Game does not exist for the user");

        // Ensure the game exists and is in correct status
        assert!(player.is_playing(), "{}", "Game is not available to play");

        if let GameStatus::WordChecked {
            correct_positions,
            contained_in_word,
            is_guessed,
        } = player.game_status.clone()
        {
            debug!("Word already checked, handling result");
            return Self::handle_word_checked(
                player,
                correct_positions,
                contained_in_word,
                is_guessed,
            );
        }

        // Validate the submitted word is in lowercase and is 5 character long
        assert!(
            word.len() == wordle_io::WORD_LENGTH,
            "{}",
            "Word must be 5 character long"
        );
        assert!(
            word.chars().all(|c| c.is_lowercase()),
            "{}",
            "Word must be lowercased"
        );

        debug!("Sending CheckWord message to wordle program");
        // Send `CheckWord` message to wordle program
        let sent_msg_id = msg::send(
            self.target_program_id,
            WordleAction::CheckWord { user, word },
            0,
        )
        .expect("Error in sending message");

        player.set_msg_ids(sent_msg_id, msg::id());
        player.game_status = GameStatus::CheckingWord;
        debug!("Word check initiated, waiting for response");

        exec::wait();
    }

    pub fn check_game_status(&mut self, user: ActorId, init_id: MessageId) {
        debug!("Checking game status for user: {:?}", user);

        assert!(
            msg::source() == exec::program_id(),
            "{}",
            "Callable by current program only"
        );

        let info = self
            .players
            .get_mut(&user)
            .expect("Player info does not exist");

        if let GameStatus::Completed(..) = info.game_status {
            debug!("Game already completed, ignoring status check");
            // ignore when game has ended
            return;
        }

        if init_id == info.init_msg_id {
            debug!("Game timed out, marking as lost");
            let game_over_status = GameOverStatus::Lose;
            info.game_status = GameStatus::Completed(game_over_status.clone());
            msg::send(user, Event::GameOver(game_over_status), 0)
                .expect("Error in sending message");
        }
    }

    fn handle_word_checked(
        player_info: &mut PlayerInfo,
        correct_positions: Vec<u8>,
        contained_in_word: Vec<u8>,
        is_guessed: bool,
    ) {
        player_info.increment_attempt();

        if is_guessed {
            return Session::complete_game(player_info, GameOverStatus::Win);
        }

        if player_info.attempts_count == 5 {
            return Session::complete_game(player_info, GameOverStatus::Lose);
        }

        Self::set_status_and_reply(
            player_info,
            GameStatus::InProgress,
            Event::WordChecked {
                correct_positions,
                contained_in_word,
            },
        )
    }

    fn complete_game(info: &mut PlayerInfo, status: GameOverStatus) {
        Self::set_status_and_reply(
            info,
            GameStatus::Completed(status.clone()),
            Event::GameOver(status),
        )
    }

    fn set_status_and_reply(info: &mut PlayerInfo, status: GameStatus, event: Event) {
        info.game_status = status;
        reply!(event)
    }
}

#[no_mangle]
extern "C" fn init() {
    let target_program_id = msg::load().expect("Unable to message's payload");
    unsafe { init_inner_state(Session::new(target_program_id)) }
}

#[no_mangle]
extern "C" fn handle() {
    let action = msg::load::<Action>().expect("Unable to message's payload");
    let session = get_inner_state_mut();

    match action {
        Action::StartGame => session.start_game(msg::source()),
        Action::CheckWord { word } => session.check_word(msg::source(), word),
        Action::CheckGameStatus { user, init_id } => session.check_game_status(user, init_id),
    }
}

#[no_mangle]
extern "C" fn handle_reply() {
    let reply_message_id = msg::reply_to().expect("Error in reading replied Message ID");

    let session = get_inner_state_mut();

    let reply_message = msg::load::<WordleEvent>().expect("Unable to message's payload");

    let user: ActorId = reply_message.clone().into();

    let player_info = session
        .players
        .get(&user)
        .expect("Player info does not exist");

    let sent_message_id = player_info.sent_msg_id();
    let original_message_id = player_info.original_msg_id();

    if reply_message_id == sent_message_id {
        let game_status: GameStatus = reply_message.into();
        session.players.entry(user).and_modify(|info| {
            info.game_status = game_status;
        });

        exec::wake(original_message_id).expect("Error in resuming paused message");
    }
}

#[no_mangle]
extern "C" fn state() {
    let state: State = get_inner_state().into();
    reply!(state)
}

impl From<Session> for State {
    fn from(value: Session) -> Self {
        Self {
            target_program_id: value.target_program_id,
            players: value.players.clone(),
        }
    }
}
