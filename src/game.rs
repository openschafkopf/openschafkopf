use card::*;
use hand::*;
use stich::*;
use rules::*;
use gamestate::*;
use player::*;
use cardvectorparser::*;
use rulesrufspiel::*;
use playercomputer::*;
use playerhuman::*;

use std::rc::Rc;
use rand::{self, Rng};

pub struct CGame {
    pub m_gamestate : SGameState,
    //m_vecplayer : Vec<Rc<CPlayer>> ,
    pub m_vecplayer : Vec<Box<CPlayer>>, // TODO: good idea to use Box<CPlayer>, maybe shared_ptr equivalent?
}

impl CGame {
    //fn new_by_random(bShort : bool/*TODO: is it a good idea to have players in CGame?*/) -> CGame; // shall replace DealCards
    pub fn new() -> CGame {
        CGame {
            m_gamestate : SGameState {
                m_ahand : {
                    let mut veccard : Vec<CCard> = Vec::new();
                    // TODO: doable via flat_map?
                    for efarbe in EFarbe::all_values().iter() {
                        for eschlag in ESchlag::all_values().iter() {
                            veccard.push(CCard::new(*efarbe, *eschlag));
                        }
                    }
                    assert!(veccard.len()==32);
                    rand::thread_rng().shuffle(&mut veccard);
                    let hand_for_player = |eplayerindex| {
                        CHand::new_from_vec(veccard.iter().cloned().skip((eplayerindex as usize)*8).take(8).collect())
                    };
                    [hand_for_player(0), hand_for_player(1), hand_for_player(2), hand_for_player(3)]
                },
                m_rules : Rc::new(CRulesRufspiel {m_eplayerindex : 0, m_efarbe: efarbeEICHEL} ),
                m_vecstich : Vec::new()
            },
            m_vecplayer : vec![ // TODO: take players in ctor?
                Box::new(CPlayerHuman),
                Box::new(CPlayerComputer),
                Box::new(CPlayerComputer),
                Box::new(CPlayerComputer)
            ]
        }
    }

    // TODO: what is a useful return value for start_game?
    pub fn start_game(&mut self, eplayerindex_first : EPlayerIndex) -> bool {
        // prepare
        self.m_gamestate.m_vecstich.clear();
        println!("Starting game");
        for hand in self.m_gamestate.m_ahand.iter() {
            print!("{} |", hand);
        }
        println!("");

        // decide which game is played
        println!("Asking players if they want to play");
        let mut vecpaireplayerindexgameannounce : Vec<(EPlayerIndex, Rc<TRules>)> = Vec::new();
        for eplayerindex in (eplayerindex_first..eplayerindex_first+4).map(|eplayerindex| eplayerindex%4) {
            if let Some(gameannounce) = self.m_vecplayer[eplayerindex].ask_for_game(
                eplayerindex,
                &self.m_gamestate.m_ahand[eplayerindex]
            ) {
                vecpaireplayerindexgameannounce.push((eplayerindex, gameannounce));
            }
        }
        if vecpaireplayerindexgameannounce.is_empty() {
            return false;
        }

        println!("Asked players if they want to play. Determining rules");
        // TODO: find sensible way to deal with multiple game announcements
        let paireplayerindexgameannounce = vecpaireplayerindexgameannounce.pop().unwrap();
        self.m_gamestate.m_rules = paireplayerindexgameannounce.1;
        println!(
            "Rules determined ({} plays {}). Sorting hands",
            paireplayerindexgameannounce.0,
            self.m_gamestate.m_rules
        );
        {
            let ref rules = self.m_gamestate.m_rules;
            for hand in self.m_gamestate.m_ahand.iter_mut() {
                hand.sort(|&card_fst, &card_snd| rules.compare_in_stich(card_fst, card_snd));
                println!("{}", hand);
            }
        }

        self.new_stich(eplayerindex_first); // install first stich
        true
    }

    pub fn which_player_can_do_something(&self) -> Option<EPlayerIndex> {
        self.m_gamestate.which_player_can_do_something()
    }

    fn new_stich(&mut self, eplayerindex_last_stich: EPlayerIndex) {
        println!("Opening new stich starting at {}", eplayerindex_last_stich);
        assert!(self.m_gamestate.m_vecstich.is_empty() || 4==self.m_gamestate.m_vecstich.last().unwrap().size());
        self.m_gamestate.m_vecstich.push(CStich::new(eplayerindex_last_stich));
        self.notify_game_listeners();
    }

    pub fn zugeben(&mut self, card_played: CCard, eplayerindex: EPlayerIndex) -> EPlayerIndex { // TODO: should invalid inputs be indicated by return value?
        // returns the EPlayerIndex of the player who is the next in row to do something
        // TODO: how to cope with finished game?
        println!("Player {} wants to play {}", eplayerindex, card_played);
        {
            let eplayerindex_privileged = self.which_player_can_do_something().unwrap();
            assert_eq!(eplayerindex, eplayerindex_privileged);
            assert!(self.m_gamestate.m_ahand[eplayerindex].contains(card_played));
        }
        {
            let ref mut hand = self.m_gamestate.m_ahand[eplayerindex];
            assert!(self.m_gamestate.m_rules.card_is_allowed(&self.m_gamestate.m_vecstich, hand, card_played));
            hand.play_card(card_played);
            self.m_gamestate.m_vecstich.last_mut().unwrap().zugeben(card_played);
        }
        for eplayerindex in 0..4 {
            println!("Hand {}: {}", eplayerindex, self.m_gamestate.m_ahand[eplayerindex]);
        }
        if 4==self.m_gamestate.m_vecstich.last().unwrap().size() {
            if 8==self.m_gamestate.m_vecstich.len() { // TODO kurze Karte?
                println!("Game finished.");
                for (i_stich, stich) in self.m_gamestate.m_vecstich.iter().enumerate() {
                    println!("Stich {}: {}", i_stich, stich);
                }
                self.notify_game_listeners();
                (self.m_gamestate.m_vecstich.first().unwrap().first_player_index() + 1) % 4 // for next game
            } else {
                // TODO: all players should have to acknowledge the current stich in some way
                let eplayerindex_last_stich = {
                    let stich = self.m_gamestate.m_vecstich.last().unwrap();
                    println!("Stich: {}", stich);
                    let eplayerindex_last_stich = self.m_gamestate.m_rules.winner_index(stich);
                    println!("{} made by {}, ({} points)",
                        stich,
                        eplayerindex_last_stich,
                        self.m_gamestate.m_rules.points_stich(stich)
                    );
                    eplayerindex_last_stich
                };
                self.new_stich(eplayerindex_last_stich);
                self.notify_game_listeners();
                eplayerindex_last_stich
            }
        } else {
            self.notify_game_listeners();
            (eplayerindex + 1) % 4
        }
    }

    pub fn points_per_player(&self) -> [isize; 4] {
        self.m_gamestate.m_rules.points_per_player(&self.m_gamestate.m_vecstich)
    }

    fn notify_game_listeners(&self) {
        // TODO: notify game listeners
    }

    pub fn payout(&self) -> [isize; 4] {
        self.m_gamestate.m_rules.payout(&self.m_gamestate.m_vecstich)
    }
    
    // fn RegisterPlayer(&mut self, Rc<CPlayer> rcplayer) -> EPlayerIndex {
    //     assert!(self.m_vecplayer.len()<4);
    //     let eplayerindex = self.m_vecplayer.len();
    //     self.m_vecplayer.push(rcplayer);
    //     eplayerindex
    // }
}
